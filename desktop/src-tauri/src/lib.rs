use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::Arc;

use agentbridge::config::{self, Config, ProxyConfig, ProxyKind};
use agentbridge::executor::{executor_definitions_with_discovery, scan_executor, test_proxy, ExecutorAvailability};
use agentbridge::projects::{self, ProjectEntry, ProjectHub};
use agentbridge::state::BridgeState;
use agentbridge::Workspace;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
struct GatewayStatus { online: bool, endpoint: String, host: String, port: u16 }

#[derive(Debug, Serialize)]
struct DashboardData { gateway: GatewayStatus, projects: Vec<projects::ProjectListing>, tasks: Vec<TaskData>, executor: String, activity: Vec<ActivityItem> }

#[derive(Debug, Serialize, Clone)]
struct TaskData { project: String, task_id: Option<String>, lifecycle: Option<String>, status: Option<String>, iteration: u32, executor: Option<String>, goal: Option<String>, summary: Option<String>, error: Option<String>, changed_files: Vec<String>, tests: Option<agentbridge::state::TestResult>, created_at: Option<String>, started_at: Option<String>, finished_at: Option<String>, updated_at: String }

#[derive(Debug, Serialize, Clone)]
struct ActivityItem { project: String, task_id: Option<String>, status: Option<String>, summary: Option<String>, timestamp: String }

#[derive(Debug, Serialize)]
struct ConnectionData { config_path: String, workspace: String, host: String, port: u16, endpoint: String, auth_enabled: bool, oauth_enabled: bool, no_auth: bool }

#[derive(Debug, Deserialize)]
struct ConfigInput { host: String, port: u16, no_auth: bool, auth_token: Option<String>, admin_password: Option<String>, executor_command: String, executor_mode: agentbridge::config::ExecutorMode }

#[derive(Debug, Deserialize)]
struct ProxyInput { enabled: bool, kind: ProxyKind, host: String, port: u16, username: Option<String>, password: Option<String> }

#[derive(Debug, Serialize)]
struct ProxyData { enabled: bool, kind: ProxyKind, host: String, port: u16, username_configured: bool, password_configured: bool }

#[derive(Debug, Deserialize)]
struct ProjectInput { id: Option<String>, name: String, path: String, executor: String }

#[derive(Debug, Deserialize)]
struct ExecutorInput { id: Option<String>, name: String, kind: String, command: String, executable: Option<String>, working_directory: Option<String>, proxy_id: Option<String>, enabled: bool }

#[derive(Debug, Serialize)]
struct ExecutorData { id: String, name: String, kind: String, command: String, executable: Option<String>, working_directory: Option<String>, proxy_id: Option<String>, enabled: bool, available: bool, version: Option<String>, error: Option<String>, status: String, detected: bool }

fn proxy_data(cfg: &ProxyConfig) -> ProxyData { ProxyData { enabled: cfg.enabled, kind: cfg.kind, host: cfg.host.clone(), port: cfg.port, username_configured: cfg.username.is_some(), password_configured: cfg.password.is_some() } }

fn load() -> Result<(Config, PathBuf), String> { config::find_config(None).map_err(|e| e.to_string()) }
fn hub(cfg: &Config, config_path: &std::path::Path) -> Result<ProjectHub, String> {
    let (entries, default) = projects::discover_from_config(config_path, &cfg.workspace)
        .map_err(|e| e.to_string())?;
    ProjectHub::open(entries, default, Arc::new(cfg.clone())).map_err(|e| e.to_string())
}

fn project_file(config_path: &std::path::Path) -> PathBuf {
    config_path.parent().unwrap_or_else(|| std::path::Path::new(".")).join("agentbridge.config.json")
}

fn project_entries(cfg: &Config, config_path: &std::path::Path) -> Result<(Vec<ProjectEntry>, Option<String>, PathBuf), String> {
    let path = project_file(config_path);
    let (entries, default) = if path.is_file() {
        projects::load_workspaces_file(&path)
    } else {
        projects::discover_from_config(config_path, &cfg.workspace)
    }.map_err(|e| e.to_string())?;
    Ok((entries, default, path))
}

fn project_list(cfg: &Config, config_path: &std::path::Path) -> Result<Vec<projects::ProjectListing>, String> {
    let hub = hub(cfg, config_path)?;
    Ok(hub.list(hub.default_name()))
}

fn executor_data(definition: agentbridge::config::ExecutorDefinition, detected: bool) -> ExecutorData {
    let availability: ExecutorAvailability = scan_executor(&definition);
    let name = if definition.display_name.trim().is_empty() { definition.name } else { definition.display_name };
    ExecutorData { id: definition.id, name, kind: definition.kind, command: definition.command, executable: definition.executable.map(|p| p.display().to_string()).or_else(|| availability.executable.map(|p| p.display().to_string())), working_directory: definition.working_directory.map(|p| p.display().to_string()), proxy_id: definition.proxy_id, enabled: definition.enabled, available: availability.available, version: availability.version, error: availability.error, status: serde_json::to_value(availability.status).unwrap().as_str().unwrap().into(), detected }
}

fn executor_list(config_path: &std::path::Path) -> Result<Vec<ExecutorData>, String> {
    let registry = config::load_executor_registry(config_path).map_err(|e| e.to_string())?;
    Ok(executor_definitions_with_discovery(&registry.executors)
        .into_iter()
        .map(|(entry, detected)| executor_data(entry, detected))
        .collect())
}
fn task_data(name: &str, workspace: &Workspace) -> Result<TaskData, String> {
    let state = BridgeState::load(workspace.root()).map_err(|e| e.to_string())?;
    Ok(TaskData { project: name.into(), task_id: state.task_id, lifecycle: state.task_status.map(|s| s.as_str().into()), status: state.status, iteration: state.iteration, executor: state.executor, goal: state.goal, summary: state.summary, error: state.error, changed_files: state.changed_files, tests: state.tests, created_at: state.created_at.map(|v| v.to_rfc3339()), started_at: state.started_at.map(|v| v.to_rfc3339()), finished_at: state.finished_at.map(|v| v.to_rfc3339()), updated_at: state.updated_at.to_rfc3339() })
}

#[tauri::command]
fn dashboard() -> Result<DashboardData, String> {
    let (cfg, config_path) = load()?; let hub = hub(&cfg, &config_path)?; let listings = hub.list(hub.default_name());
    let mut tasks = Vec::new(); let mut activity = Vec::new();
    for name in hub.names() { if let Some(project) = hub.get(name) { let task = task_data(name, &project.workspace)?; if task.task_id.is_some() { activity.push(ActivityItem { project: task.project.clone(), task_id: task.task_id.clone(), status: task.status.clone(), summary: task.summary.clone(), timestamp: task.updated_at.clone() }); } tasks.push(task); } }
    let online = TcpStream::connect((cfg.host.as_str(), cfg.port)).is_ok();
    Ok(DashboardData { gateway: GatewayStatus { online, endpoint: cfg.mcp_url(), host: cfg.host, port: cfg.port }, projects: listings, tasks, executor: cfg.executor.kind, activity })
}

#[tauri::command]
fn connection() -> Result<ConnectionData, String> { let (cfg, path) = load()?; Ok(ConnectionData { config_path: path.display().to_string(), workspace: cfg.workspace.display().to_string(), host: cfg.host.clone(), port: cfg.port, endpoint: cfg.mcp_url(), auth_enabled: cfg.auth_token.is_some(), oauth_enabled: cfg.admin_password.is_some(), no_auth: cfg.no_auth }) }

#[tauri::command]
fn settings() -> Result<serde_json::Value, String> { let (cfg, _) = load()?; let prefs = config::load_ui_prefs(); Ok(serde_json::json!({"executor_command": cfg.executor.command, "executor_mode": cfg.executor.mode, "auto_start": prefs.auto_start, "start_tunnel": prefs.start_tunnel, "notifications": false})) }

#[tauri::command]
fn proxy() -> Result<ProxyData, String> { let (cfg, _) = load()?; Ok(proxy_data(&cfg.proxy)) }

#[tauri::command]
fn available_executors() -> Result<Vec<String>, String> { let (cfg, _) = load()?; Ok(vec![cfg.executor.kind]) }

#[tauri::command]
fn executors() -> Result<Vec<ExecutorData>, String> { let (_, path) = load()?; executor_list(&path) }

#[tauri::command]
fn choose_executor_file() -> Option<String> { rfd::FileDialog::new().pick_file().map(|path| path.display().to_string()) }

#[tauri::command]
fn choose_executor_directory() -> Option<String> { rfd::FileDialog::new().pick_folder().map(|path| path.display().to_string()) }

#[tauri::command]
fn save_executor(input: ExecutorInput) -> Result<Vec<ExecutorData>, String> {
    let (_, path) = load()?;
    let mut registry = config::load_executor_registry(&path).map_err(|e| e.to_string())?;
    if input.name.trim().is_empty() || input.command.trim().is_empty() { return Err("name and command are required".into()); }
    let mut definition = agentbridge::config::ExecutorDefinition::new(input.name.trim().into(), input.kind.trim().to_ascii_lowercase(), input.command.trim().into());
    definition.display_name = input.name.trim().into();
    definition.id = input.id.filter(|id| !id.trim().is_empty()).unwrap_or(definition.id);
    definition.executable = input.executable.filter(|v| !v.trim().is_empty()).map(PathBuf::from);
    definition.working_directory = input.working_directory.filter(|v| !v.trim().is_empty()).map(PathBuf::from);
    definition.proxy_id = input.proxy_id.filter(|v| !v.trim().is_empty());
    definition.enabled = input.enabled;
    if let Some(existing) = registry.executors.iter_mut().find(|entry| entry.id == definition.id) { *existing = definition; } else { registry.executors.push(definition); }
    config::save_executor_registry(&path, &registry).map_err(|e| e.to_string())?;
    executor_list(&path)
}

#[tauri::command]
fn delete_executor(id: String) -> Result<Vec<ExecutorData>, String> { let (_, path) = load()?; let mut registry = config::load_executor_registry(&path).map_err(|e| e.to_string())?; let before = registry.executors.len(); registry.executors.retain(|entry| entry.id != id); if registry.executors.len() == before { return Err("executor not found".into()); } config::save_executor_registry(&path, &registry).map_err(|e| e.to_string())?; executor_list(&path) }

#[tauri::command]
fn test_executor(id: String) -> Result<ExecutorData, String> { let (_, path) = load()?; let registry = config::load_executor_registry(&path).map_err(|e| e.to_string())?; let (entry, detected) = executor_definitions_with_discovery(&registry.executors).into_iter().find(|(entry, _)| entry.id == id).ok_or_else(|| "executor not found".to_string())?; Ok(executor_data(entry, detected)) }

#[tauri::command]
fn choose_project_directory() -> Option<String> { rfd::FileDialog::new().pick_folder().map(|path| path.display().to_string()) }

#[tauri::command]
fn save_project(input: ProjectInput) -> Result<Vec<projects::ProjectListing>, String> {
    let (cfg, config_path) = load()?;
    let (mut entries, mut default, path) = project_entries(&cfg, &config_path)?;
    let name = projects::validate_project_name(&input.name).map_err(|e| e.to_string())?;
    let project_path = std::path::absolute(PathBuf::from(input.path.trim())).map_err(|e| e.to_string())?;
    if !project_path.is_dir() { return Err(format!("project path is not a directory: {}", project_path.display())); }
    let executor = input.executor.trim().to_ascii_lowercase();
    if executor.is_empty() { return Err("executor is required".into()); }
    if let Some(id) = input.id.filter(|value| !value.trim().is_empty()) {
        if entries.iter().any(|other| other.id != id && other.name.eq_ignore_ascii_case(&name)) { return Err("project name already exists".into()); }
        let entry = entries.iter_mut().find(|entry| entry.id == id).ok_or_else(|| "project not found".to_string())?;
        entry.name = name;
        entry.path = project_path;
        entry.executor = executor;
    } else {
        if entries.iter().any(|entry| entry.name.eq_ignore_ascii_case(&name)) { return Err("project name already exists".into()); }
        entries.push(ProjectEntry { id: String::new(), name, path: project_path, description: String::new(), readonly: false, executor });
        if default.is_none() { default = entries.last().map(|entry| entry.name.clone()); }
    }
    projects::save_workspaces_file(&path, &entries, default).map_err(|e| e.to_string())?;
    project_list(&cfg, &config_path)
}

#[tauri::command]
fn delete_project(id: String) -> Result<Vec<projects::ProjectListing>, String> {
    let (cfg, config_path) = load()?;
    let (mut entries, mut default, path) = project_entries(&cfg, &config_path)?;
    if entries.len() <= 1 { return Err("at least one project must remain".into()); }
    let index = entries.iter().position(|entry| entry.id == id).ok_or_else(|| "project not found".to_string())?;
    let removed = entries.remove(index);
    if default.as_deref() == Some(removed.name.as_str()) { default = entries.first().map(|entry| entry.name.clone()); }
    projects::save_workspaces_file(&path, &entries, default).map_err(|e| e.to_string())?;
    project_list(&cfg, &config_path)
}

#[tauri::command]
fn save_proxy(input: ProxyInput) -> Result<ProxyData, String> { let (mut cfg, path) = load()?; let mut next = ProxyConfig { enabled: input.enabled, kind: input.kind, host: input.host.trim().into(), port: input.port, username: input.username.filter(|v| !v.trim().is_empty()), password: input.password.filter(|v| !v.trim().is_empty()) }; if next.username.is_none() { next.username = cfg.proxy.username.clone(); } if next.password.is_none() { next.password = cfg.proxy.password.clone(); } next.validate().map_err(|e| e.to_string())?; cfg.proxy = next; cfg.save_to_path(&path).map_err(|e| e.to_string())?; Ok(proxy_data(&cfg.proxy)) }

#[tauri::command]
async fn test_proxy_connection(input: ProxyInput) -> Result<String, String> { let proxy = ProxyConfig { enabled: true, kind: input.kind, host: input.host.trim().into(), port: input.port, username: input.username.filter(|v| !v.trim().is_empty()), password: input.password.filter(|v| !v.trim().is_empty()) }; test_proxy(&proxy).await.map_err(|e| e.to_string()) }

#[tauri::command]
fn save_connection(input: ConfigInput) -> Result<ConnectionData, String> { let (mut cfg, path) = load()?; if input.host.trim().is_empty() || input.port == 0 { return Err("host and port are required".into()); } cfg.host = input.host.trim().into(); cfg.port = input.port; cfg.no_auth = input.no_auth; if let Some(token) = input.auth_token.filter(|v| !v.trim().is_empty()) { cfg.auth_token = Some(token); } if let Some(password) = input.admin_password.filter(|v| !v.trim().is_empty()) { cfg.admin_password = Some(password); } cfg.executor.command = input.executor_command.trim().into(); cfg.executor.mode = input.executor_mode; cfg.save_to_path(&path).map_err(|e| e.to_string())?; connection() }

#[tauri::command]
async fn cancel_task(project_name: String, task_id: Option<String>) -> Result<TaskData, String> { let (cfg, config_path) = load()?; let hub = hub(&cfg, &config_path)?; let project = hub.get(&project_name).ok_or_else(|| "project not found".to_string())?; let runtime = project.runtime.clone(); runtime.cancel(task_id.as_deref()).await.map_err(|e| e.to_string())?; task_data(&project.name, &project.workspace) }

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![dashboard, connection, settings, proxy, available_executors, executors, choose_executor_file, choose_executor_directory, save_executor, delete_executor, test_executor, choose_project_directory, save_project, delete_project, save_proxy, test_proxy_connection, save_connection, cancel_task])
        .run(tauri::generate_context!())
        .expect("error while running AgentBridge desktop");
}
