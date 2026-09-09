use agentbridge::config;
use agentbridge::server::{self, ServeHandle, ServeOptions};
use agentbridge::state::BridgeState;
use agentbridge::Workspace;
use std::net::TcpStream;
use std::sync::Mutex;
use tauri::State;

use crate::commands::{hub, load};
use crate::dto::{
    ActivityItem, ConfigInput, ConnectionData, DashboardData, GatewayStatus, TaskData,
};

// 托管网关句柄状态
pub struct GatewayState(pub Mutex<Option<ServeHandle>>);

pub fn probe_gateway(host: &str, port: u16) -> bool {
    TcpStream::connect((host, port)).is_ok()
}

pub fn task_data(name: &str, workspace: &Workspace) -> Result<TaskData, String> {
    let state = BridgeState::load(workspace.root()).map_err(|e| e.to_string())?;
    Ok(TaskData {
        project: name.into(),
        task_id: state.task_id,
        lifecycle: state.task_status.map(|s| s.as_str().into()),
        status: state.status,
        iteration: state.iteration,
        executor: state.executor,
        goal: state.goal,
        summary: state.summary,
        error: state.error,
        changed_files: state.changed_files,
        tests: state.tests,
        created_at: state.created_at.map(|v| v.to_rfc3339()),
        started_at: state.started_at.map(|v| v.to_rfc3339()),
        finished_at: state.finished_at.map(|v| v.to_rfc3339()),
        updated_at: state.updated_at.to_rfc3339(),
    })
}

#[tauri::command]
pub fn dashboard(state: State<GatewayState>) -> Result<DashboardData, String> {
    let (cfg, config_path) = load()?;
    let hub = hub(&cfg, &config_path)?;
    let listings = hub.list(hub.default_name());
    let mut tasks = Vec::new();
    let mut activity = Vec::new();

    for name in hub.names() {
        if let Some(project) = hub.get(&name) {
            let task = task_data(&name, &project.workspace)?;
            // 只有真正执行过任务的项目，才记录进活动流水
            if let Some(task_id) = &task.task_id {
                let display_summary = if let Some(summary) = &task.summary {
                    summary.clone()
                } else if task.lifecycle.as_deref() == Some("running") {
                    "AI 执行器已启动，正在本地工作区实时编写代码与执行测试...".to_string()
                } else {
                    task.goal.clone().unwrap_or_else(|| "任务状态已更新".to_string())
                };

                activity.push(ActivityItem {
                    project: task.project.clone(),
                    task_id: Some(task_id.clone()),
                    status: task.status.clone().or_else(|| task.lifecycle.clone()),
                    goal: task.goal.clone(),
                    summary: Some(display_summary),
                    timestamp: task.updated_at.clone(),
                });
            }
            tasks.push(task);
        }
    }

    // 核心对齐：活动流与任务列表均按最新时间倒序排列
    activity.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    tasks.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    let clients = if let Some(handle) = state.0.lock().unwrap().as_ref() {
        handle.oauth.list_connected_clients()
    } else {
        Vec::new()
    };

    let online = probe_gateway(&cfg.host, cfg.port);
    let managed = state.0.lock().unwrap().is_some();

    Ok(DashboardData {
        gateway: GatewayStatus {
            online,
            endpoint: cfg.mcp_url(),
            host: cfg.host,
            port: cfg.port,
            managed,
            clients,
        },
        projects: listings,
        tasks,
        executor: cfg.executor.kind,
        activity,
    })
}

#[tauri::command]
pub fn start_gateway(state: State<GatewayState>) -> Result<GatewayStatus, String> {
    let (cfg, config_path) = load()?;
    let mut guard = state.0.lock().unwrap();

    // 1. 如果本桌面端已经托管了，直接取它的 clients 返回
    if let Some(handle) = guard.as_ref() {
        let clients = handle.oauth.list_connected_clients();
        return Ok(GatewayStatus {
            online: true,
            endpoint: cfg.mcp_url(),
            host: cfg.host,
            port: cfg.port,
            managed: true,
            clients,
        });
    }

    // 2. 如果端口已经被外部占用（例如 CLI 已经在跑）
    if probe_gateway(&cfg.host, cfg.port) {
        return Ok(GatewayStatus {
            online: true,
            endpoint: cfg.mcp_url(),
            host: cfg.host,
            port: cfg.port,
            managed: false,
            clients: Vec::new(),
        });
    }

    // 3. 正常拉起后台 Core 网关，严格读取配置
    let hub = hub(&cfg, &config_path)?;
    let options = ServeOptions {
        allow_any_host: cfg.allow_any_host,
        no_auth: cfg.no_auth,
        client_id: cfg.client_id.clone(),
        client_secret: cfg.client_secret.clone(),
        admin_password: cfg.admin_password.clone(),
    };

    let handle = server::spawn_server(cfg.clone(), hub, options).map_err(|e| e.to_string())?;
    let clients = handle.oauth.list_connected_clients();
    *guard = Some(handle);

    Ok(GatewayStatus {
        online: true,
        endpoint: cfg.mcp_url(),
        host: cfg.host,
        port: cfg.port,
        managed: true,
        clients,
    })
}

#[tauri::command]
pub fn stop_gateway(state: State<GatewayState>) -> Result<GatewayStatus, String> {
    let (cfg, _) = load()?;
    let mut guard = state.0.lock().unwrap();

    if let Some(mut handle) = guard.take() {
        handle.stop();
    }

    let online = probe_gateway(&cfg.host, cfg.port);
    Ok(GatewayStatus {
        online,
        endpoint: cfg.mcp_url(),
        host: cfg.host,
        port: cfg.port,
        managed: false,
        clients: Vec::new(),
    })
}

#[tauri::command]
pub fn connection() -> Result<ConnectionData, String> {
    let (cfg, path) = load()?;
    Ok(ConnectionData {
        config_path: path.display().to_string(),
        workspace: cfg.workspace.display().to_string(),
        host: cfg.host.clone(),
        port: cfg.port,
        endpoint: cfg.mcp_url(),
        auth_enabled: cfg.auth_token.is_some(),
        oauth_enabled: cfg.admin_password.is_some(),
        no_auth: cfg.no_auth,
    })
}

#[tauri::command]
pub fn settings() -> Result<serde_json::Value, String> {
    let (cfg, _) = load()?;
    let prefs = config::load_ui_prefs();
    Ok(serde_json::json!({
        "executor_command": cfg.executor.command,
        "executor_mode": cfg.executor.mode,
        "auto_start": prefs.auto_start,
        "start_tunnel": prefs.start_tunnel,
        "notifications": false
    }))
}

#[tauri::command]
pub fn save_connection(input: ConfigInput) -> Result<ConnectionData, String> {
    let (mut cfg, path) = load()?;
    if input.host.trim().is_empty() || input.port == 0 {
        return Err("host and port are required".into());
    }
    cfg.host = input.host.trim().into();
    cfg.port = input.port;
    cfg.no_auth = input.no_auth;
    if let Some(token) = input.auth_token.filter(|v| !v.trim().is_empty()) {
        cfg.auth_token = Some(token);
    }
    if let Some(password) = input.admin_password.filter(|v| !v.trim().is_empty()) {
        cfg.admin_password = Some(password);
    }
    cfg.executor.command = input.executor_command.trim().into();
    cfg.executor.mode = input.executor_mode;
    cfg.save_to_path(&path).map_err(|e| e.to_string())?;
    connection()
}