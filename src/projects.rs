//! Multi-project workspace hub.
//!
//! A single AgentBridge process can mount several local repositories and
//! switch the active project per MCP session. Every file/git/executor call
//! is sandboxed to the selected project's root.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::config::Config;
use crate::task::TaskRuntime;
use crate::workspace::Workspace;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkspacesFile {
    pub projects: Vec<ProjectEntry>,
    #[serde(default)]
    pub default_project: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProjectEntry {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub readonly: bool,
    #[serde(default = "default_project_executor")]
    pub executor: String,
}

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

pub struct ProjectHub {
    projects: Vec<ProjectHandle>,
    by_name: HashMap<String, usize>,
    default_name: String,
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
    pub fn open(
        entries: Vec<ProjectEntry>,
        default_name: Option<String>,
        config: Arc<Config>,
    ) -> Result<Self> {
        if entries.is_empty() {
            bail!("no projects configured");
        }
        let mut projects = Vec::new();
        let mut by_name = HashMap::new();
        for entry in entries {
            let id = project_id(&entry);
            let name = validate_project_name(&entry.name)?;
            if by_name.contains_key(&name) {
                bail!("duplicate project name `{name}`");
            }
            if !entry.path.is_dir() {
                bail!(
                    "project `{name}` path does not exist or is not a directory: {}",
                    entry.path.display()
                );
            }
            let workspace = Workspace::open(
                &entry.path,
                config.security.max_file_size,
                config.security.deny_sensitive_files,
            )
            .with_context(|| format!("cannot open project `{name}`"))?;
            let workspace = Arc::new(workspace);
            let runtime = Arc::new(
                TaskRuntime::new(workspace.clone(), config.clone())
                    .with_context(|| format!("invalid executor configuration for `{name}`"))?,
            );
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
        let default_name = match default_name {
            Some(name) => {
                if !by_name.contains_key(&name) {
                    bail!("default_project `{name}` is not in the project list");
                }
                name
            }
            None => projects[0].name.clone(),
        };
        Ok(Self {
            projects,
            by_name,
            default_name,
        })
    }

    pub fn single(path: PathBuf, config: Arc<Config>) -> Result<Self> {
        Self::open(
            vec![ProjectEntry {
                id: String::new(),
                name: "default".into(),
                path,
                description: "Default workspace".into(),
                readonly: false,
                executor: default_project_executor(),
            }],
            Some("default".into()),
            config,
        )
    }

    pub fn get(&self, name: &str) -> Option<&ProjectHandle> {
        let name = name.trim();
        if let Some(idx) = self.by_name.get(name) {
            return self.projects.get(*idx);
        }
        self.projects
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
    }

    pub fn default_name(&self) -> &str {
        &self.default_name
    }

    pub fn names(&self) -> Vec<&str> {
        self.projects.iter().map(|p| p.name.as_str()).collect()
    }

    pub fn len(&self) -> usize {
        self.projects.len()
    }

    pub fn list(&self, active: &str) -> Vec<ProjectListing> {
        self.projects
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

/// Resolve the set of projects from CLI flags, config files, and the
/// existing single-workspace Config.
pub fn discover(
    workspaces_flag: Option<&Path>,
    positional: Option<&Path>,
    workspace_flag: Option<&Path>,
    config_workspace: &Path,
) -> Result<(Vec<ProjectEntry>, Option<String>)> {
    if let Some(path) = workspaces_flag {
        return load_workspaces_file(path);
    }
    if let Some(dir) = positional {
        let dir = std::path::absolute(dir)?;
        if !dir.is_dir() {
            bail!("workspace does not exist: {}", dir.display());
        }
        return Ok((
            vec![ProjectEntry {
                name: "default".into(),
                path: dir,
                description: "Default workspace".into(),
                readonly: false,
                id: String::new(),
                executor: default_project_executor(),
            }],
            Some("default".into()),
        ));
    }
    if let Some(dir) = workspace_flag {
        let dir = std::path::absolute(dir)?;
        if !dir.is_dir() {
            bail!("workspace does not exist: {}", dir.display());
        }
        return Ok((
            vec![ProjectEntry {
                name: "default".into(),
                path: dir,
                description: "Default workspace".into(),
                readonly: false,
                id: String::new(),
                executor: default_project_executor(),
            }],
            Some("default".into()),
        ));
    }
    let cwd_file = PathBuf::from("agentbridge.config.json");
    if cwd_file.is_file() {
        return load_workspaces_file(&cwd_file);
    }
    Ok((
        vec![ProjectEntry {
            name: "default".into(),
            path: config_workspace.to_path_buf(),
            description: "Default workspace".into(),
            readonly: false,
            id: String::new(),
            executor: default_project_executor(),
        }],
        Some("default".into()),
    ))
}

/// Discover projects for a desktop config, where the process working
/// directory is not necessarily the directory containing the config.
pub fn discover_from_config(
    config_path: &Path,
    config_workspace: &Path,
) -> Result<(Vec<ProjectEntry>, Option<String>)> {
    let workspaces = config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("agentbridge.config.json");
    if workspaces.is_file() {
        return load_workspaces_file(&workspaces);
    }
    Ok((
        vec![ProjectEntry {
            name: "default".into(),
            path: config_workspace.to_path_buf(),
            description: "Default workspace".into(),
            readonly: false,
            id: String::new(),
            executor: default_project_executor(),
        }],
        Some("default".into()),
    ))
}

pub fn save_workspaces_file(
    path: &Path,
    projects: &[ProjectEntry],
    default_project: Option<String>,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let mut projects = projects.to_vec();
    for project in &mut projects {
        project.name = validate_project_name(&project.name)?;
        if project.id.trim().is_empty() {
            project.id = project_id(project);
        }
        if project.executor.trim().is_empty() {
            project.executor = default_project_executor();
        }
    }
    let file = WorkspacesFile {
        projects,
        default_project,
    };
    let text = serde_json::to_string_pretty(&file).context("failed to serialize workspaces")?;
    fs::write(path, text + "\n").with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub fn load_workspaces_file(path: &Path) -> Result<(Vec<ProjectEntry>, Option<String>)> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let file: WorkspacesFile = serde_json::from_str(&text)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    if file.projects.is_empty() {
        bail!("{} contains no projects", path.display());
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut entries = Vec::new();
    for mut entry in file.projects {
        entry.path = expand_path(&entry.path);
        if !entry.path.is_absolute() {
            entry.path = parent.join(&entry.path);
        }
        entry.path = std::path::absolute(&entry.path)
            .with_context(|| format!("invalid project path {}", entry.path.display()))?;
        if entry.id.trim().is_empty() {
            entry.id = project_id(&entry);
        }
        if entry.executor.trim().is_empty() {
            entry.executor = default_project_executor();
        }
        entries.push(entry);
    }
    Ok((entries, file.default_project))
}

pub fn validate_project_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        bail!("project name must not be empty");
    }
    if name.len() > 64 {
        bail!("project name is too long");
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_alphanumeric() {
        bail!("project name `{name}` must start with a letter or digit");
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')) {
        bail!("project name `{name}` may only contain letters, digits, '.', '-' and '_'");
    }
    Ok(name.to_string())
}

fn expand_path(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if s == "~" {
        return dirs::home_dir().unwrap_or_else(|| path.to_path_buf());
    }
    if let Some(rest) = s.strip_prefix("~/").or_else(|| s.strip_prefix("~\\")) {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn validates_names() {
        assert!(validate_project_name("indexflow-core").is_ok());
        assert!(validate_project_name("a").is_ok());
        assert!(validate_project_name("../secret").is_err());
        assert!(validate_project_name("has space").is_err());
        assert!(validate_project_name("").is_err());
    }

    #[test]
    fn loads_json_and_opens_two_projects() {
        let a = TempDir::new().unwrap();
        let b = TempDir::new().unwrap();
        fs::write(a.path().join("a.txt"), "A").unwrap();
        fs::write(b.path().join("b.txt"), "B").unwrap();
        let cfg_dir = TempDir::new().unwrap();
        let json = cfg_dir.path().join("agentbridge.config.json");
        let body = serde_json::json!({
            "projects": [
                {"name": "alpha", "path": a.path(), "description": "A"},
                {"name": "beta", "path": b.path(), "description": "B", "readonly": true}
            ],
            "default_project": "alpha"
        });
        fs::write(&json, body.to_string()).unwrap();
        let (entries, default) = load_workspaces_file(&json).unwrap();
        assert_eq!(default.as_deref(), Some("alpha"));
        let cfg = Arc::new(Config::new(a.path().to_path_buf()));
        let hub = ProjectHub::open(entries, default, cfg).unwrap();
        assert_eq!(hub.len(), 2);
        assert_eq!(hub.default_name(), "alpha");
        let alpha = hub.get("alpha").unwrap();
        let beta = hub.get("BETA").unwrap();
        assert_eq!(alpha.workspace.read_file("a.txt").unwrap(), "A");
        assert!(alpha.workspace.read_file("../b.txt").is_err());
        assert_eq!(beta.workspace.read_file("b.txt").unwrap(), "B");
        assert!(beta.readonly);
        assert!(beta.workspace.read_file("a.txt").is_err());
    }

    #[test]
    fn single_dir_named_default() {
        let dir = TempDir::new().unwrap();
        let cfg = Arc::new(Config::new(dir.path().to_path_buf()));
        let hub = ProjectHub::single(dir.path().to_path_buf(), cfg).unwrap();
        assert_eq!(hub.names(), vec!["default"]);
    }

    #[test]
    fn workspaces_file_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("agentbridge.config.json");
        let entries = vec![ProjectEntry {
            name: "alpha".into(),
            path: dir.path().to_path_buf(),
            description: "A".into(),
            readonly: false,
            id: String::new(),
            executor: default_project_executor(),
        }];
        save_workspaces_file(&path, &entries, Some("alpha".into())).unwrap();
        let (loaded, default) = load_workspaces_file(&path).unwrap();
        assert_eq!(default.as_deref(), Some("alpha"));
        assert_eq!(loaded[0].name, "alpha");
    }

    #[test]
    fn discovers_workspaces_next_to_config_file() {
        let config_dir = TempDir::new().unwrap();
        let alpha = TempDir::new().unwrap();
        let beta = TempDir::new().unwrap();
        let config_path = config_dir.path().join("config.toml");
        let workspaces_path = config_dir.path().join("agentbridge.config.json");
        fs::write(
            &workspaces_path,
            serde_json::json!({
                "projects": [
                    {"name": "alpha", "path": alpha.path()},
                    {"name": "beta", "path": beta.path()}
                ],
                "default_project": "beta"
            })
            .to_string(),
        )
        .unwrap();

        let (entries, default) = discover_from_config(&config_path, alpha.path()).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(default.as_deref(), Some("beta"));
    }
}

pub fn default_project_executor() -> String { "opencode".into() }

pub fn project_id(entry: &ProjectEntry) -> String {
    if !entry.id.trim().is_empty() {
        return entry.id.trim().to_string();
    }
    let mut hash = Sha256::new();
    hash.update(entry.name.trim().as_bytes());
    hash.update([0]);
    hash.update(entry.path.to_string_lossy().as_bytes());
    let digest = format!("{:x}", hash.finalize());
    format!("project-{}", &digest[..24])
}
