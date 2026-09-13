use std::net::TcpStream;

use anyhow::Result;
use serde::Serialize;

use crate::config::Config;
use crate::executor::{scan_executor, ExecutorAvailability};
use crate::oauth::ConnectedClientInfo;
use crate::projects::{ProjectHub, ProjectListing};
use crate::state::{BridgeState, TestResult};
use crate::workspace::Workspace;

const RUNNING_ACTIVITY_SUMMARY: &str =
    "AI 执行器已启动，正在本地工作区实时编写代码与执行测试...";

#[derive(Debug, Clone, Serialize)]
pub struct GatewayStatus {
    pub online: bool,
    pub endpoint: String,
    pub host: String,
    pub port: u16,
    pub managed: bool,
    pub clients: Vec<ConnectedClientInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutorStatus {
    pub kind: String,
    pub command: String,
    pub available: bool,
    pub implemented: bool,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskSnapshot {
    pub project: String,
    pub task_id: Option<String>,
    pub lifecycle: Option<String>,
    pub status: Option<String>,
    pub iteration: u32,
    pub executor: Option<String>,
    pub goal: Option<String>,
    pub summary: Option<String>,
    pub error: Option<String>,
    pub changed_files: Vec<String>,
    pub tests: Option<TestResult>,
    pub created_at: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActivityItem {
    pub project: String,
    pub task_id: Option<String>,
    pub status: Option<String>,
    pub goal: Option<String>,
    pub summary: Option<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardSnapshot {
    pub gateway: GatewayStatus,
    pub projects: Vec<ProjectListing>,
    pub project_count: usize,
    pub default_project: String,
    pub tasks: Vec<TaskSnapshot>,
    pub executor: ExecutorStatus,
    pub activity: Vec<ActivityItem>,
}

pub fn probe_gateway(host: &str, port: u16) -> bool {
    TcpStream::connect((host, port)).is_ok()
}

pub fn gateway_status(
    cfg: &Config,
    managed: bool,
    clients: Vec<ConnectedClientInfo>,
) -> GatewayStatus {
    gateway_status_with_online(cfg, probe_gateway(&cfg.host, cfg.port), managed, clients)
}

pub fn gateway_status_with_online(
    cfg: &Config,
    online: bool,
    managed: bool,
    clients: Vec<ConnectedClientInfo>,
) -> GatewayStatus {
    GatewayStatus {
        online,
        endpoint: cfg.mcp_url(),
        host: cfg.host.clone(),
        port: cfg.port,
        managed,
        clients,
    }
}

pub fn task_snapshot(name: &str, workspace: &Workspace) -> Result<TaskSnapshot> {
    let state = BridgeState::load(workspace.root())?;
    Ok(TaskSnapshot {
        project: name.to_string(),
        task_id: state.task_id,
        lifecycle: state.task_status.map(|s| s.as_str().to_string()),
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

pub fn executor_status(cfg: &Config) -> ExecutorStatus {
    let mut definition = crate::config::ExecutorDefinition::new(
        cfg.executor.kind.clone(),
        cfg.executor.kind.clone(),
        cfg.executor.command.clone(),
    );
    definition.id = format!("default-{}", cfg.executor.kind);
    let availability: ExecutorAvailability = scan_executor(&definition);
    ExecutorStatus {
        kind: cfg.executor.kind.clone(),
        command: cfg.executor.command.clone(),
        available: availability.available,
        implemented: cfg.executor.kind.eq_ignore_ascii_case("opencode"),
        version: availability.version,
    }
}

pub fn snapshot(
    cfg: &Config,
    hub: &ProjectHub,
    gateway: GatewayStatus,
) -> Result<DashboardSnapshot> {
    let default_project = hub.default_name();
    let projects = hub.list(&default_project);
    let mut tasks = Vec::new();
    let mut activity = Vec::new();

    for name in hub.names() {
        let Some(project) = hub.get(&name) else {
            continue;
        };
        let task = task_snapshot(&name, &project.workspace)?;
        if let Some(task_id) = &task.task_id {
            let display_summary = if let Some(summary) = &task.summary {
                summary.clone()
            } else if task.lifecycle.as_deref() == Some("running") {
                RUNNING_ACTIVITY_SUMMARY.to_string()
            } else {
                task.goal
                    .clone()
                    .unwrap_or_else(|| "任务状态已更新".to_string())
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

    activity.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    tasks.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    Ok(DashboardSnapshot {
        gateway,
        project_count: projects.len(),
        default_project,
        projects,
        tasks,
        executor: executor_status(cfg),
        activity,
    })
}
