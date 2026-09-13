use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

use crate::projects::entry::{default_project_executor, validate_project_name, ProjectEntry};
use crate::projects::file::{
    load_workspaces_file, save_workspaces_file, PROJECTS_JSON_LEGACY, PROJECTS_TOML_FILE,
};

/// Sidecar projects file next to a config file.
/// Prefers `projects.toml` if it exists, otherwise `agentbridge.config.json`.
/// If neither exists, new writes create the JSON file (current desktop behavior).
pub fn projects_file_for_config(config_path: &Path) -> PathBuf {
    let parent = config_path.parent().unwrap_or_else(|| Path::new("."));
    let toml = parent.join(PROJECTS_TOML_FILE);
    if toml.is_file() {
        return toml;
    }
    parent.join(PROJECTS_JSON_LEGACY)
}

pub fn load_projects_or_empty(path: &Path) -> Result<(Vec<ProjectEntry>, Option<String>)> {
    if path.is_file() {
        load_workspaces_file(path)
    } else {
        Ok((Vec::new(), None))
    }
}

#[derive(Debug, Clone, Default)]
pub struct ProjectUpsert {
    /// Some = update this id (insert with this id if missing, matching current desktop).
    pub id: Option<String>,
    pub name: String,
    pub path: PathBuf,
    pub description: Option<String>,
    pub executor: Option<String>,
    pub readonly: Option<bool>,
    pub make_default: bool,
}

/// Single Core write path for project create/update.
pub fn upsert_project(
    file: &Path,
    input: ProjectUpsert,
) -> Result<(Vec<ProjectEntry>, Option<String>)> {
    let name = validate_project_name(&input.name)?;
    let path = std::path::absolute(&input.path)?;
    if !path.is_dir() {
        bail!("project path is not a directory: {}", path.display());
    }

    let (mut entries, mut default) = load_projects_or_empty(file)?;
    let executor = input
        .executor
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_lowercase());

    if let Some(id) = input.id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        if entries
            .iter()
            .any(|other| other.id != id && other.name.eq_ignore_ascii_case(&name))
        {
            bail!("project name already exists");
        }
        if let Some(entry) = entries.iter_mut().find(|entry| entry.id == id) {
            let old_name = entry.name.clone();
            entry.name = name.clone();
            entry.path = path;
            if let Some(description) = input.description {
                entry.description = description;
            }
            if let Some(executor) = executor {
                entry.executor = executor;
            }
            if let Some(readonly) = input.readonly {
                entry.readonly = readonly;
            }
            if default.as_deref() == Some(old_name.as_str()) {
                default = Some(name.clone());
            }
        } else {
            entries.push(ProjectEntry {
                id: id.to_string(),
                name: name.clone(),
                path,
                description: input.description.unwrap_or_default(),
                readonly: input.readonly.unwrap_or(false),
                executor: executor.unwrap_or_else(default_project_executor),
            });
        }
    } else {
        if entries
            .iter()
            .any(|entry| entry.name.eq_ignore_ascii_case(&name))
        {
            bail!("project `{name}` already exists");
        }
        entries.push(ProjectEntry {
            id: String::new(),
            name: name.clone(),
            path,
            description: input.description.unwrap_or_default(),
            readonly: input.readonly.unwrap_or(false),
            executor: executor.unwrap_or_else(default_project_executor),
        });
    }

    if input.make_default || default.is_none() {
        default = Some(name);
    }

    save_workspaces_file(file, &entries, default.clone())?;
    load_workspaces_file(file)
}

/// Remove by stable id or name. Refuses to delete the last project.
pub fn remove_project(
    file: &Path,
    id_or_name: &str,
) -> Result<(Vec<ProjectEntry>, Option<String>)> {
    let needle = id_or_name.trim();
    if needle.is_empty() {
        bail!("project id or name is required");
    }
    let (mut entries, mut default) = load_workspaces_file(file)?;
    if entries.len() <= 1 {
        bail!("at least one project must remain");
    }
    let index = entries
        .iter()
        .position(|entry| {
            entry.id == needle || entry.name.eq_ignore_ascii_case(needle)
        })
        .ok_or_else(|| anyhow::anyhow!("project `{needle}` was not found"))?;
    let removed = entries.remove(index);
    if default.as_deref() == Some(removed.name.as_str())
        || default
            .as_deref()
            .is_some_and(|d| d.eq_ignore_ascii_case(needle))
    {
        default = entries.first().map(|entry| entry.name.clone());
    }
    save_workspaces_file(file, &entries, default.clone())?;
    load_workspaces_file(file)
}
