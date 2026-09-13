use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::projects::entry::{default_project_executor, project_id, validate_project_name, ProjectEntry};

pub const PROJECTS_TOML_FILE: &str = "projects.toml";
pub const PROJECTS_JSON_LEGACY: &str = "agentbridge.config.json";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkspacesFile {
    pub projects: Vec<ProjectEntry>,
    #[serde(default)]
    pub default_project: Option<String>,
}

pub fn discover(
    workspaces_flag: Option<&Path>,
    positional: Option<&Path>,
    workspace_flag: Option<&Path>,
    config_workspace: &Path,
    // When Some, project files are resolved next to this config (`--config`).
    // When None, keep the historical CWD projects.toml / agentbridge.config.json lookup.
    config_path: Option<&Path>,
) -> Result<(Vec<ProjectEntry>, Option<String>)> {
    if let Some(path) = workspaces_flag {
        return load_workspaces_file(path);
    }
    if let Some(dir) = positional.or(workspace_flag) {
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
    if let Some(config_path) = config_path {
        return discover_from_config(config_path, config_workspace);
    }
    let toml_file = PathBuf::from(PROJECTS_TOML_FILE);
    if toml_file.is_file() {
        return load_workspaces_file(&toml_file);
    }
    let json_file = PathBuf::from(PROJECTS_JSON_LEGACY);
    if json_file.is_file() {
        return load_workspaces_file(&json_file);
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

pub fn discover_from_config(
    config_path: &Path,
    config_workspace: &Path,
) -> Result<(Vec<ProjectEntry>, Option<String>)> {
    let parent = config_path.parent().unwrap_or_else(|| Path::new("."));
    let toml_file = parent.join(PROJECTS_TOML_FILE);
    if toml_file.is_file() {
        return load_workspaces_file(&toml_file);
    }
    let json_file = parent.join(PROJECTS_JSON_LEGACY);
    if json_file.is_file() {
        return load_workspaces_file(&json_file);
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
        fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
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
    let file = WorkspacesFile { projects, default_project };
    let text = if path.extension().is_some_and(|ext| ext == "toml") {
        toml::to_string_pretty(&file).context("failed to serialize projects.toml")?
    } else {
        serde_json::to_string_pretty(&file).context("failed to serialize workspaces")?
    };
    fs::write(path, text + "\n").with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub fn load_workspaces_file(path: &Path) -> Result<(Vec<ProjectEntry>, Option<String>)> {
    let text = fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let file: WorkspacesFile = if path.extension().is_some_and(|ext| ext == "toml") {
        toml::from_str(&text).with_context(|| format!("failed to parse TOML {}", path.display()))?
    } else {
        serde_json::from_str(&text).with_context(|| format!("failed to parse JSON {}", path.display()))?
    };
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
        entry.path = std::path::absolute(&entry.path).with_context(|| format!("invalid path {}", entry.path.display()))?;
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