// src/projects/mod.rs
pub mod entry;
use anyhow::{bail, Result};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::config::Config;
use crate::executor::{shared_registry, ExecutorRegistry, SharedExecutorRegistry};
use crate::storage::Storage;
use crate::task::TaskRuntime;
use crate::workspace::Workspace;

pub use entry::{default_project_executor, project_id, validate_project_name, ProjectEntry};

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
}

impl HubState {
    fn build(
        entries: Vec<ProjectEntry>,
        default_name: String,
        config: &Config,
        registry: &SharedExecutorRegistry,
        storage: &Arc<Storage>,
    ) -> Result<Self> {
        if entries.is_empty() {
            bail!("no projects configured");
        }
        let mut projects = Vec::new();
        let mut by_name = HashMap::new();
        for entry in entries {
            let id = entry.id.clone();
            let name = entry.name.clone();
            let workspace = Arc::new(Workspace::open(
                &entry.path,
                config.security.max_file_size,
                config.security.deny_sensitive_files,
            )?);
            let default_executor = if entry.executor.trim().is_empty() {
                default_project_executor()
            } else {
                entry.executor.clone()
            };
            let runtime = Arc::new(TaskRuntime::new(
                name.clone(),
                workspace.clone(),
                default_executor,
                registry.clone(),
                config.executor.mode,
                storage.clone(),
            )?);
            by_name.insert(name.clone(), projects.len());
            projects.push(ProjectHandle {
                id,
                name,
                description: entry.description,
                readonly: entry.readonly,
                executor: entry.executor,
                workspace,
                runtime,
            });
        }
        Ok(Self {
            projects,
            by_name,
            default_name,
        })
    }
}

pub struct ProjectHub {
    state: RwLock<HubState>,
    config: Arc<Config>,
    registry: SharedExecutorRegistry,
    storage: Arc<Storage>,
}

#[derive(Debug, Clone, serde::Serialize)]
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
    pub fn new(config: Arc<Config>, storage: Arc<Storage>) -> Result<Self> {
        let registry = shared_registry(ExecutorRegistry::from_config(
            &config,
            &storage.load_executors()?,
        )?);
        let mut entries = storage.load_projects()?;
        if entries.is_empty() {
            entries.push(ProjectEntry {
                id: "default".into(),
                name: "default".into(),
                path: std::env::current_dir()?,
                description: String::new(),
                readonly: false,
                executor: "opencode".into(),
            });
        }
        let default_name = entries.first().map(|e| e.name.clone()).unwrap();
        let state = HubState::build(entries, default_name, &config, &registry, &storage)?;
        Ok(Self {
            state: RwLock::new(state),
            config,
            registry,
            storage,
        })
    }

    /// Rebuild the runtime [`ExecutorRegistry`] from the SQLite executor store.
    ///
    /// The registry is swapped behind the shared handle, so already-built
    /// `TaskRuntime` values observe the new configuration on their next task.
    /// The default `Config.executor` (OpenCode) is always re-registered by
    /// `ExecutorRegistry::from_config`, preserving existing semantics.
    pub fn reload_executors(&self) -> Result<bool> {
        let definitions = self.storage.load_executors()?;
        let registry = ExecutorRegistry::from_config(&self.config, &definitions)?;
        *self.registry.write().unwrap() = Arc::new(registry);
        Ok(true)
    }

    pub fn reload(&self) -> Result<bool> {
        let mut entries = self.storage.load_projects()?;
        if entries.is_empty() {
            entries.push(ProjectEntry {
                id: "default".into(),
                name: "default".into(),
                path: std::env::current_dir()?,
                description: String::new(),
                readonly: false,
                executor: "opencode".into(),
            });
        }
        let default_name = entries.first().map(|e| e.name.clone()).unwrap();
        let new_state = HubState::build(
            entries,
            default_name,
            &self.config,
            &self.registry,
            &self.storage,
        )?;
        *self.state.write().unwrap() = new_state;
        Ok(true)
    }

    pub fn get(&self, name: impl AsRef<str>) -> Option<ProjectHandle> {
        let name = name.as_ref().trim();
        if let Ok(lock) = self.state.read() {
            // 修复点：直接解包获取 usize 索引，不要借用临时变量
            let idx_opt = lock.by_name.get(name).copied().or_else(|| {
                lock.projects
                    .iter()
                    .position(|p| p.name.eq_ignore_ascii_case(name))
            });
            if let Some(idx) = idx_opt {
                return Some(lock.projects[idx].clone());
            }
        }
        None
    }

    pub fn default_name(&self) -> String {
        self.state
            .read()
            .map(|s| s.default_name.clone())
            .unwrap_or_else(|_| "default".into())
    }
    pub fn names(&self) -> Vec<String> {
        self.state
            .read()
            .map(|s| s.projects.iter().map(|p| p.name.clone()).collect())
            .unwrap_or_default()
    }
    pub fn len(&self) -> usize {
        self.state.read().map(|s| s.projects.len()).unwrap_or(0)
    }

    pub fn list(&self, active: impl AsRef<str>) -> Vec<ProjectListing> {
        let active = active.as_ref();
        let lock = match self.state.read() {
            Ok(l) => l,
            Err(_) => return Vec::new(),
        };
        lock.projects
            .iter()
            .map(|p| {
                let info = p.workspace.info();
                ProjectListing {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    path: info.workspace,
                    description: p.description.clone(),
                    readonly: p.readonly,
                    active: p.name == active,
                    git_repository: info.git_repository,
                    project_type: info.project_type,
                    executor: p.executor.clone(),
                }
            })
            .collect()
    }
}
