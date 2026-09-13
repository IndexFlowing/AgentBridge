pub mod entry;
pub mod file;
pub mod store;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::SystemTime;
use anyhow::{bail, Result};
use serde::Serialize;

use crate::config::{self, Config};
use crate::executor::ExecutorRegistry;
use crate::task::TaskRuntime;
use crate::workspace::Workspace;

pub use entry::{default_project_executor, project_id, validate_project_name, ProjectEntry};
pub use file::{discover, discover_from_config, load_workspaces_file, save_workspaces_file, PROJECTS_JSON_LEGACY, PROJECTS_TOML_FILE, WorkspacesFile};
pub use store::{
    load_projects_or_empty, projects_file_for_config, remove_project, upsert_project, ProjectUpsert,
};

#[derive(Clone)]
pub struct ProjectHandle {
    pub id: String,
    pub name: String,
    pub description: String,
    pub readonly: bool,
    pub executor: String,
    pub workspace: Arc<Workspace>,
    pub runtime: Arc<TaskRuntime>,
}

struct HubState {
    projects: Vec<ProjectHandle>,
    by_name: HashMap<String, usize>,
    default_name: String,
    last_mtime: Option<SystemTime>,
}

impl HubState {
    fn build(
        entries: Vec<ProjectEntry>,
        default_name: Option<String>,
        config: &Config,
        registry: &Arc<ExecutorRegistry>,
        mtime: Option<SystemTime>,
    ) -> Result<Self> {
        if entries.is_empty() { bail!("no projects configured"); }
        let mut projects = Vec::new();
        let mut by_name = HashMap::new();
        for entry in entries {
            let id = project_id(&entry);
            let name = validate_project_name(&entry.name)?;
            if by_name.contains_key(&name) { bail!("duplicate project name `{name}`"); }
            if !entry.path.is_dir() { bail!("project `{name}` path does not exist: {}", entry.path.display()); }
            let workspace = Arc::new(Workspace::open(&entry.path, config.security.max_file_size, config.security.deny_sensitive_files)?);
            let default_executor = if entry.executor.trim().is_empty() { default_project_executor() } else { entry.executor.clone() };
            let runtime = Arc::new(TaskRuntime::new(workspace.clone(), default_executor, registry.clone(), config.executor.mode)?);
            by_name.insert(name.clone(), projects.len());
            projects.push(ProjectHandle { id, name, description: entry.description, readonly: entry.readonly, executor: entry.executor, workspace, runtime });
        }
        let default_name = default_name.filter(|d| by_name.contains_key(d)).unwrap_or_else(|| projects[0].name.clone());
        Ok(Self { projects, by_name, default_name, last_mtime: mtime })
    }
}

pub struct ProjectHub {
    state: RwLock<HubState>,
    config: Arc<Config>,
    registry: Arc<ExecutorRegistry>,
    config_path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectListing {
    pub id: String,
    pub name: String,
    pub path: String,
    pub description: String,
    pub readonly: bool,
    pub active: bool,
    pub git_repository: bool,
    pub project_type: Vec<String>,
    pub executor: String,
}

impl ProjectHub {
    pub fn open(entries: Vec<ProjectEntry>, default_name: Option<String>, config: Arc<Config>) -> Result<Self> {
        let config_path = config::find_config(None).map(|(_, p)| p).unwrap_or_else(|_| config::project_config_path());
        Self::open_with_path(entries, default_name, config, config_path)
    }

    pub fn open_with_path(entries: Vec<ProjectEntry>, default_name: Option<String>, config: Arc<Config>, config_path: PathBuf) -> Result<Self> {
        let executors_file = config::load_executor_registry(&config_path).unwrap_or_default();
        let registry = Arc::new(ExecutorRegistry::from_config(&config, &executors_file.executors)?);
        let mtime = get_file_mtime(&config_path);
        let state = HubState::build(entries, default_name, &config, &registry, mtime)?;
        Ok(Self { state: RwLock::new(state), config, registry, config_path })
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub fn has_executor(&self, id_or_name: &str) -> bool {
        self.registry.get(id_or_name).is_some()
    }

    pub fn single(path: PathBuf, config: Arc<Config>) -> Result<Self> {
        Self::open(vec![ProjectEntry { id: String::new(), name: "default".into(), path, description: "Default workspace".into(), readonly: false, executor: default_project_executor() }], Some("default".into()), config)
    }

    /// 智能热重载：仅当磁盘上的配置文件发生实际变化时才重新加载
    pub fn reload(&self) -> Result<bool> {
        let resolved_path = self.resolve_active_config_path();
        let current_mtime = get_file_mtime(&resolved_path);

        // 如果文件修改时间未变，说明磁盘无改动，直接返回，不重复构建也不刷日志
        if let Ok(lock) = self.state.read() {
            if current_mtime.is_some() && lock.last_mtime == current_mtime {
                return Ok(false);
            }
        }

        let (entries, default) = discover_from_config(&resolved_path, &self.config.workspace)?;

        // 防御性保护：防止空配置冲掉已有的多项目
        if entries.len() == 1 && entries[0].name == "default" {
            let current_len = self.state.read().map(|s| s.projects.len()).unwrap_or(0);
            if current_len > 1 {
                return Ok(false);
            }
        }

        let new_state = HubState::build(entries, default, &self.config, &self.registry, current_mtime)?;
        eprintln!(
            "[Projects] 检测到配置文件变动，已同步 {} 个项目: {:?} (默认: '{}')",
            new_state.projects.len(),
            new_state.by_name.keys().collect::<Vec<_>>(),
            new_state.default_name
        );
        let mut lock = self.state.write().map_err(|e| anyhow::anyhow!("Lock error: {e}"))?;
        *lock = new_state;
        Ok(true)
    }

    pub fn get(&self, name: impl AsRef<str>) -> Option<ProjectHandle> {
        let name = name.as_ref().trim();
        let find_idx = |lock: &HubState| -> Option<usize> {
            lock.by_name.get(name).copied().or_else(|| {
                lock.projects.iter().position(|p| p.name.eq_ignore_ascii_case(name))
            })
        };

        if let Ok(lock) = self.state.read() {
            if let Some(idx) = find_idx(&lock) {
                return Some(lock.projects[idx].clone());
            }
        }
        // 如果未命中，尝试自适应刷新一次
        if self.reload().unwrap_or(false) {
            if let Ok(lock) = self.state.read() {
                if let Some(idx) = find_idx(&lock) {
                    return Some(lock.projects[idx].clone());
                }
            }
        }
        None
    }

    pub fn default_name(&self) -> String {
        self.state.read().map(|s| s.default_name.clone()).unwrap_or_else(|_| "default".into())
    }

    pub fn names(&self) -> Vec<String> {
        self.state.read().map(|s| s.projects.iter().map(|p| p.name.clone()).collect()).unwrap_or_default()
    }

    pub fn len(&self) -> usize {
        self.state.read().map(|s| s.projects.len()).unwrap_or(0)
    }

    pub fn list(&self, active: impl AsRef<str>) -> Vec<ProjectListing> {
        let active = active.as_ref();
        let _ = self.reload(); // 仅当文件修改时才真正做操作
        let lock = match self.state.read() { Ok(l) => l, Err(_) => return Vec::new() };
        lock.projects.iter().map(|p| {
            let info = p.workspace.info();
            ProjectListing {
                id: p.id.clone(), name: p.name.clone(), path: info.workspace, description: p.description.clone(),
                readonly: p.readonly, active: p.name == active, git_repository: info.git_repository, project_type: info.project_type, executor: p.executor.clone(),
            }
        }).collect()
    }

    fn resolve_active_config_path(&self) -> PathBuf {
        self.config_path.clone()
    }
}

fn get_file_mtime(config_path: &Path) -> Option<SystemTime> {
    let parent = config_path.parent().unwrap_or_else(|| Path::new("."));
    let toml = parent.join(PROJECTS_TOML_FILE);
    if toml.is_file() {
        return fs::metadata(&toml).ok().and_then(|m| m.modified().ok());
    }
    let json = parent.join(PROJECTS_JSON_LEGACY);
    if json.is_file() {
        return fs::metadata(&json).ok().and_then(|m| m.modified().ok());
    }
    fs::metadata(config_path).ok().and_then(|m| m.modified().ok())
}