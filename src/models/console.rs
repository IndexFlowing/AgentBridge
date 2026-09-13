// src/models/console.rs
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::config::{Config, ExecutorMode, ProxyConfig, ProxyKind};
use crate::dashboard::{
    ActivityItem as CoreActivityItem, DashboardSnapshot, GatewayStatus as CoreGatewayStatus,
    TaskSnapshot,
};
use crate::executor::ExecutorView;
use crate::projects::ProjectListing;
use crate::state::TestResult;

#[derive(Debug, Serialize)]
pub struct GatewayStatus {
    pub online: bool,
    pub endpoint: String,
    pub host: String,
    pub port: u16,
    pub managed: bool,
    pub clients: Vec<crate::oauth::ConnectedClientInfo>,
}

impl From<CoreGatewayStatus> for GatewayStatus {
    fn from(core: CoreGatewayStatus) -> Self {
        Self {
            online: core.online,
            endpoint: core.endpoint,
            host: core.host,
            port: core.port,
            managed: core.managed,
            clients: core.clients,
        }
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct TaskData {
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

impl From<TaskSnapshot> for TaskData {
    fn from(t: TaskSnapshot) -> Self {
        Self {
            project: t.project,
            task_id: t.task_id,
            lifecycle: t.lifecycle,
            status: t.status,
            iteration: t.iteration,
            executor: t.executor,
            goal: t.goal,
            summary: t.summary,
            error: t.error,
            changed_files: t.changed_files,
            tests: t.tests,
            created_at: t.created_at,
            started_at: t.started_at,
            finished_at: t.finished_at,
            updated_at: t.updated_at,
        }
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct ActivityItem {
    pub project: String,
    pub task_id: Option<String>,
    pub status: Option<String>,
    pub goal: Option<String>,
    pub summary: Option<String>,
    pub timestamp: String,
}

impl From<CoreActivityItem> for ActivityItem {
    fn from(core: CoreActivityItem) -> Self {
        Self {
            project: core.project,
            task_id: core.task_id,
            status: core.status,
            goal: core.goal,
            summary: core.summary,
            timestamp: core.timestamp,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DashboardData {
    pub gateway: GatewayStatus,
    pub projects: Vec<ProjectListing>,
    pub tasks: Vec<TaskData>,
    pub executor: String,
    pub activity: Vec<ActivityItem>,
}

impl From<DashboardSnapshot> for DashboardData {
    fn from(snap: DashboardSnapshot) -> Self {
        Self {
            gateway: GatewayStatus::from(snap.gateway),
            projects: snap.projects,
            tasks: snap.tasks.into_iter().map(TaskData::from).collect(),
            executor: snap.executor.kind,
            activity: snap.activity.into_iter().map(ActivityItem::from).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ConnectionData {
    pub config_path: String,
    pub workspace: String,
    pub host: String,
    pub port: u16,
    pub endpoint: String,
    pub auth_enabled: bool,
    pub oauth_enabled: bool,
    pub no_auth: bool,
}

impl ConnectionData {
    pub fn new(cfg: &Config, path: PathBuf) -> Self {
        Self {
            config_path: path.display().to_string(),
            workspace: cfg.workspace.display().to_string(),
            host: cfg.host.clone(),
            port: cfg.port,
            endpoint: cfg.mcp_url(),
            auth_enabled: cfg.auth_token.is_some(),
            oauth_enabled: cfg.admin_password.is_some(),
            no_auth: cfg.no_auth,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ConfigInput {
    pub host: String,
    pub port: u16,
    pub no_auth: bool,
    #[serde(default)]
    pub allow_any_host: Option<bool>,
    pub auth_token: Option<String>,
    pub admin_password: Option<String>,
    pub executor_command: String,
    pub executor_mode: ExecutorMode,
}

#[derive(Debug, Deserialize)]
pub struct ProxyInput {
    pub enabled: bool,
    pub kind: ProxyKind,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProxyData {
    pub enabled: bool,
    pub kind: ProxyKind,
    pub host: String,
    pub port: u16,
    pub username_configured: bool,
    pub password_configured: bool,
}

impl From<&ProxyConfig> for ProxyData {
    fn from(cfg: &ProxyConfig) -> Self {
        let view = crate::config::proxy_view(cfg);
        Self {
            enabled: view.enabled,
            kind: view.kind,
            host: view.host,
            port: view.port,
            username_configured: view.username_configured,
            password_configured: view.password_configured,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ProjectInput {
    pub id: Option<String>,
    pub name: String,
    pub path: String,
    pub executor: String,
}

#[derive(Debug, Deserialize)]
pub struct ExecutorInput {
    pub id: Option<String>,
    pub name: String,
    pub kind: String,
    pub command: String,
    pub executable: Option<String>,
    pub working_directory: Option<String>,
    pub proxy_id: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct ExecutorData {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub command: String,
    pub executable: Option<String>,
    pub working_directory: Option<String>,
    pub proxy_id: Option<String>,
    pub enabled: bool,
    pub available: bool,
    pub version: Option<String>,
    pub error: Option<String>,
    pub status: String,
    pub detected: bool,
}

impl From<ExecutorView> for ExecutorData {
    fn from(view: ExecutorView) -> Self {
        let name = if view.definition.display_name.trim().is_empty() {
            view.definition.name
        } else {
            view.definition.display_name
        };
        Self {
            id: view.definition.id,
            name,
            kind: view.definition.kind,
            command: view.definition.command,
            executable: view
                .definition
                .executable
                .map(|p| p.display().to_string())
                .or_else(|| {
                    view.availability
                        .executable
                        .map(|p| p.display().to_string())
                }),
            working_directory: view
                .definition
                .working_directory
                .map(|p| p.display().to_string()),
            proxy_id: view.definition.proxy_id,
            enabled: view.definition.enabled,
            available: view.availability.available,
            version: view.availability.version,
            error: view.availability.error,
            status: view.availability.status.as_str().to_string(),
            detected: view.detected,
        }
    }
}
