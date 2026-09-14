// src/dashboard.rs
use anyhow::Result;
use serde::Serialize;

use crate::config::Config;
use crate::oauth::ConnectedClientInfo;
use crate::projects::{ProjectHub, ProjectListing};
use crate::state::TestResult;
use crate::storage::Storage;

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

pub fn gateway_status(
    cfg: &Config,
    managed: bool,
    clients: Vec<ConnectedClientInfo>,
) -> GatewayStatus {
    GatewayStatus {
        online: true,
        endpoint: cfg.mcp_url(),
        host: cfg.host.clone(),
        port: cfg.port,
        managed,
        clients,
    }
}

pub fn task_snapshot(name: &str, storage: &Storage) -> Result<TaskSnapshot> {
    let state = storage.load_task_state(name).unwrap_or_default();
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

pub fn snapshot(
    cfg: &Config,
    hub: &ProjectHub,
    gateway: GatewayStatus,
    storage: &Storage,
) -> Result<DashboardSnapshot> {
    let default_project = hub.default_name();
    let projects = hub.list(&default_project);
    let mut tasks = Vec::new();
    let mut activity = Vec::new();

    for name in hub.names() {
        let task = task_snapshot(&name, storage)?;
        if let Some(task_id) = &task.task_id {
            activity.push(ActivityItem {
                project: task.project.clone(),
                task_id: Some(task_id.clone()),
                status: task.status.clone(),
                goal: task.goal.clone(),
                summary: task.summary.clone(),
                timestamp: task.updated_at.clone(),
            });
        }
        tasks.push(task);
    }

    Ok(DashboardSnapshot {
        gateway,
        project_count: projects.len(),
        default_project,
        projects,
        tasks,
        executor: ExecutorStatus {
            kind: cfg.executor.kind.clone(),
            command: cfg.executor.command.clone(),
            available: true,
            implemented: true,
            version: None,
        },
        activity,
    })
}
