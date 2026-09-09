use agentbridge::config::{ExecutorMode, ProxyKind};
use agentbridge::projects::ProjectListing;
use agentbridge::state::TestResult;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct GatewayStatus {
    pub online: bool,
    pub endpoint: String,
    pub host: String,
    pub port: u16,
    pub managed: bool,
    pub clients: Vec<agentbridge::oauth::server::ConnectedClientInfo>,
}

#[derive(Debug, Serialize)]
pub struct DashboardData {
    pub gateway: GatewayStatus,
    pub projects: Vec<ProjectListing>,
    pub tasks: Vec<TaskData>,
    pub executor: String,
    pub activity: Vec<ActivityItem>,
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

#[derive(Debug, Serialize, Clone)]
pub struct ActivityItem {
    pub project: String,
    pub task_id: Option<String>,
    pub status: Option<String>,
    pub goal: Option<String>,
    pub summary: Option<String>,
    pub timestamp: String,
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

#[derive(Debug, Deserialize)]
pub struct ConfigInput {
    pub host: String,
    pub port: u16,
    pub no_auth: bool,
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