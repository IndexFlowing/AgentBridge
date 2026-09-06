use std::path::{Path, PathBuf};
use agentbridge::config::{self, ExecutorDefinition};
use agentbridge::executor::{executor_definitions_with_discovery, scan_executor, ExecutorAvailability};

use crate::commands::load;
use crate::dto::{ExecutorData, ExecutorInput};

fn executor_data(definition: ExecutorDefinition, detected: bool) -> ExecutorData {
    let availability: ExecutorAvailability = scan_executor(&definition);
    let name = if definition.display_name.trim().is_empty() { definition.name } else { definition.display_name };
    ExecutorData {
        id: definition.id,
        name,
        kind: definition.kind,
        command: definition.command,
        executable: definition.executable.map(|p| p.display().to_string()).or_else(|| availability.executable.map(|p| p.display().to_string())),
        working_directory: definition.working_directory.map(|p| p.display().to_string()),
        proxy_id: definition.proxy_id,
        enabled: definition.enabled,
        available: availability.available,
        version: availability.version,
        error: availability.error,
        status: availability.status.as_str().to_string(),
        detected,
    }
}

fn executor_list(config_path: &Path) -> Result<Vec<ExecutorData>, String> {
    let registry = config::load_executor_registry(config_path).map_err(|e| e.to_string())?;
    Ok(executor_definitions_with_discovery(&registry.executors)
        .into_iter()
        .map(|(entry, detected)| executor_data(entry, detected))
        .collect())
}

#[tauri::command]
pub fn available_executors() -> Result<Vec<String>, String> {
    let (_, path) = load()?;
    let registry = config::load_executor_registry(&path).map_err(|e| e.to_string())?;
    let entries = executor_definitions_with_discovery(&registry.executors);
    let mut names = Vec::new();

    for (def, _) in entries {
        let availability = scan_executor(&def);
        // 👈 将本地探测可用或启用的执行器全部加入可选列表
        if availability.available || def.enabled {
            if !names.contains(&def.kind) {
                names.push(def.kind);
            }
        }
    }

    if names.is_empty() {
        names.push("opencode".into());
    }
    Ok(names)
}

#[tauri::command]
pub fn executors() -> Result<Vec<ExecutorData>, String> {
    let (_, path) = load()?;
    executor_list(&path)
}

#[tauri::command]
pub fn choose_executor_file() -> Option<String> {
    rfd::FileDialog::new().pick_file().map(|path| path.display().to_string())
}

#[tauri::command]
pub fn choose_executor_directory() -> Option<String> {
    rfd::FileDialog::new().pick_folder().map(|path| path.display().to_string())
}

#[tauri::command]
pub fn save_executor(input: ExecutorInput) -> Result<Vec<ExecutorData>, String> {
    let (_, path) = load()?;
    let mut registry = config::load_executor_registry(&path).map_err(|e| e.to_string())?;
    if input.name.trim().is_empty() || input.command.trim().is_empty() {
        return Err("name and command are required".into());
    }
    let mut definition = ExecutorDefinition::new(input.name.trim().into(), input.kind.trim().to_ascii_lowercase(), input.command.trim().into());
    definition.display_name = input.name.trim().into();
    definition.id = input.id.filter(|id| !id.trim().is_empty()).unwrap_or(definition.id);
    definition.executable = input.executable.filter(|v| !v.trim().is_empty()).map(PathBuf::from);
    definition.working_directory = input.working_directory.filter(|v| !v.trim().is_empty()).map(PathBuf::from);
    definition.proxy_id = input.proxy_id.filter(|v| !v.trim().is_empty());
    definition.enabled = input.enabled;
    if let Some(existing) = registry.executors.iter_mut().find(|entry| entry.id == definition.id) {
        *existing = definition;
    } else {
        registry.executors.push(definition);
    }
    config::save_executor_registry(&path, &registry).map_err(|e| e.to_string())?;
    executor_list(&path)
}

#[tauri::command]
pub fn delete_executor(id: String) -> Result<Vec<ExecutorData>, String> {
    let (_, path) = load()?;
    let mut registry = config::load_executor_registry(&path).map_err(|e| e.to_string())?;
    let before = registry.executors.len();
    registry.executors.retain(|entry| entry.id != id);
    if registry.executors.len() == before {
        return Err("executor not found".into());
    }
    config::save_executor_registry(&path, &registry).map_err(|e| e.to_string())?;
    executor_list(&path)
}

#[tauri::command]
pub fn test_executor(id: String) -> Result<ExecutorData, String> {
    let (_, path) = load()?;
    let registry = config::load_executor_registry(&path).map_err(|e| e.to_string())?;
    let (entry, detected) = executor_definitions_with_discovery(&registry.executors)
        .into_iter()
        .find(|(entry, _)| entry.id == id)
        .ok_or_else(|| "executor not found".to_string())?;
    Ok(executor_data(entry, detected))
}