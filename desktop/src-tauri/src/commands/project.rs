use std::path::{Path, PathBuf};
use agentbridge::config::Config;
use agentbridge::projects::{self, ProjectEntry, ProjectListing};

use crate::commands::{hub, load};
use crate::dto::ProjectInput;

fn project_file(config_path: &Path) -> PathBuf {
    config_path.parent().unwrap_or_else(|| Path::new(".")).join("agentbridge.config.json")
}

fn project_entries(cfg: &Config, config_path: &Path) -> Result<(Vec<ProjectEntry>, Option<String>, PathBuf), String> {
    let path = project_file(config_path);
    let (entries, default) = if path.is_file() {
        projects::load_workspaces_file(&path)
    } else {
        projects::discover_from_config(config_path, &cfg.workspace)
    }.map_err(|e| e.to_string())?;
    Ok((entries, default, path))
}

fn project_list(cfg: &Config, config_path: &Path) -> Result<Vec<ProjectListing>, String> {
    let hub = hub(cfg, config_path)?;
    Ok(hub.list(hub.default_name()))
}

#[tauri::command]
pub fn choose_project_directory() -> Option<String> {
    rfd::FileDialog::new().pick_folder().map(|path| path.display().to_string())
}

#[tauri::command]
pub fn save_project(input: ProjectInput) -> Result<Vec<ProjectListing>, String> {
    let (cfg, config_path) = load()?;
    let (mut entries, mut default, path) = project_entries(&cfg, &config_path)?;
    let name = projects::validate_project_name(&input.name).map_err(|e| e.to_string())?;
    let project_path = std::path::absolute(PathBuf::from(input.path.trim())).map_err(|e| e.to_string())?;
    if !project_path.is_dir() {
        return Err(format!("project path is not a directory: {}", project_path.display()));
    }
    let executor = input.executor.trim().to_ascii_lowercase();
    if executor.is_empty() {
        return Err("executor is required".into());
    }
    if let Some(id) = input.id.filter(|value| !value.trim().is_empty()) {
        if entries.iter().any(|other| other.id != id && other.name.eq_ignore_ascii_case(&name)) {
            return Err("project name already exists".into());
        }
        let entry = entries.iter_mut().find(|entry| entry.id == id).ok_or_else(|| "project not found".to_string())?;
        entry.name = name;
        entry.path = project_path;
        entry.executor = executor;
    } else {
        if entries.iter().any(|entry| entry.name.eq_ignore_ascii_case(&name)) {
            return Err("project name already exists".into());
        }
        entries.push(ProjectEntry {
            id: String::new(),
            name,
            path: project_path,
            description: String::new(),
            readonly: false,
            executor,
        });
        if default.is_none() {
            default = entries.last().map(|entry| entry.name.clone());
        }
    }
    projects::save_workspaces_file(&path, &entries, default).map_err(|e| e.to_string())?;
    project_list(&cfg, &config_path)
}

#[tauri::command]
pub fn delete_project(id: String) -> Result<Vec<ProjectListing>, String> {
    let (cfg, config_path) = load()?;
    let (mut entries, mut default, path) = project_entries(&cfg, &config_path)?;
    if entries.len() <= 1 {
        return Err("at least one project must remain".into());
    }
    let index = entries.iter().position(|entry| entry.id == id).ok_or_else(|| "project not found".to_string())?;
    let removed = entries.remove(index);
    if default.as_deref() == Some(removed.name.as_str()) {
        default = entries.first().map(|entry| entry.name.clone());
    }
    projects::save_workspaces_file(&path, &entries, default).map_err(|e| e.to_string())?;
    project_list(&cfg, &config_path)
}