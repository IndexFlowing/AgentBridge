// src/mcp/mod.rs
pub mod schema;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerInfo},
    ErrorData as McpError, RoleServer, ServerHandler,
};
use rmcp::{tool, tool_handler, tool_router};
use std::sync::{Arc, Mutex};

use crate::config::Config;
use crate::git;
use crate::mcp::schema::*;
use crate::projects::{ProjectHandle, ProjectHub};
use crate::storage::Storage;
use crate::task::PlanInput;
use crate::workspace::{default_search_limit, WorkspaceError};

const INSTRUCTIONS: &str =
    "You are the Brain. Inspect with read-only tools. The Executor (OpenCode) edits files.";

#[derive(Clone)]
pub struct AgentBridgeMcp {
    pub hub: Arc<ProjectHub>,
    pub config: Arc<Config>,
    pub storage: Arc<Storage>, // <--- 注入 Storage
    pub active_project: Arc<Mutex<String>>,
}

impl AgentBridgeMcp {
    pub fn new(hub: Arc<ProjectHub>, config: Arc<Config>, storage: Arc<Storage>) -> Self {
        let active = hub.default_name();
        Self {
            hub,
            config,
            storage,
            active_project: Arc::new(Mutex::new(active)),
        }
    }
    pub fn active_name(&self) -> String {
        self.active_project.lock().unwrap().clone()
    }
    pub fn project(&self, req: Option<&str>) -> Result<ProjectHandle, String> {
        let name = match req.map(str::trim).filter(|s| !s.is_empty()) {
            Some(n) => n.to_string(),
            None => self.active_name(),
        };
        self.hub
            .get(&name)
            .ok_or_else(|| format!("unknown project `{name}`"))
    }
}

#[tool_router]
impl AgentBridgeMcp {
    #[tool(description = "List mounted projects.")]
    fn list_projects(&self) -> Result<CallToolResult, McpError> {
        json_ok(
            &serde_json::json!({ "projects": self.hub.list(self.active_name()), "active_project": self.active_name() }),
        )
    }

    #[tool(description = "Switch active project.")]
    fn switch_project(
        &self,
        Parameters(args): Parameters<SwitchProjectArgs>,
    ) -> Result<CallToolResult, McpError> {
        let Some(p) = self.hub.get(&args.project_name) else {
            return tool_err_msg(format!("unknown project `{}`", args.project_name));
        };
        *self.active_project.lock().unwrap() = p.name.clone();
        json_ok(&serde_json::json!({ "active_project": p.name }))
    }

    #[tool(description = "Return workspace info.")]
    fn workspace_info(
        &self,
        Parameters(args): Parameters<ProjectArgs>,
    ) -> Result<CallToolResult, McpError> {
        let p = match self.project(args.project.as_deref()) {
            Ok(p) => p,
            Err(e) => return tool_err_msg(e),
        };
        json_ok(&p.workspace.info())
    }

    #[tool(description = "List a directory.")]
    fn list_directory(
        &self,
        Parameters(args): Parameters<PathArgs>,
    ) -> Result<CallToolResult, McpError> {
        let p = match self.project(args.project.as_deref()) {
            Ok(p) => p,
            Err(e) => return tool_err_msg(e),
        };
        match p.workspace.list_directory(&args.path) {
            Ok(l) => json_ok(&l),
            Err(e) => tool_err(e),
        }
    }

    #[tool(description = "Read a file.")]
    fn read_file(
        &self,
        Parameters(args): Parameters<ReadFileArgs>,
    ) -> Result<CallToolResult, McpError> {
        let p = match self.project(args.project.as_deref()) {
            Ok(p) => p,
            Err(e) => return tool_err_msg(e),
        };
        match p.workspace.read_file(&args.path) {
            Ok(c) => json_ok(&serde_json::json!({ "content": c })),
            Err(e) => tool_err(e),
        }
    }

    #[tool(description = "Search files.")]
    fn search_workspace(
        &self,
        Parameters(args): Parameters<SearchArgs>,
    ) -> Result<CallToolResult, McpError> {
        let p = match self.project(args.project.as_deref()) {
            Ok(p) => p,
            Err(e) => return tool_err_msg(e),
        };
        match p.workspace.search(&args.query, default_search_limit()) {
            Ok(r) => json_ok(&r),
            Err(e) => tool_err(e),
        }
    }

    #[tool(description = "Return git status.")]
    fn git_status(
        &self,
        Parameters(args): Parameters<ProjectArgs>,
    ) -> Result<CallToolResult, McpError> {
        let p = match self.project(args.project.as_deref()) {
            Ok(p) => p,
            Err(e) => return tool_err_msg(e),
        };
        match git::status(p.workspace.root()) {
            Ok(s) => json_ok(&s),
            Err(e) => tool_err_msg(e.to_string()),
        }
    }

    #[tool(description = "Return git diff.")]
    fn git_diff(
        &self,
        Parameters(args): Parameters<GitDiffArgs>,
    ) -> Result<CallToolResult, McpError> {
        let p = match self.project(args.project.as_deref()) {
            Ok(p) => p,
            Err(e) => return tool_err_msg(e),
        };
        match git::diff(
            p.workspace.root(),
            args.staged,
            self.config.security.max_diff_bytes,
        ) {
            Ok(d) => json_ok(&d),
            Err(e) => tool_err_msg(e.to_string()),
        }
    }

    #[tool(description = "Return the latest test result.")]
    fn test_status(
        &self,
        Parameters(args): Parameters<ProjectArgs>,
    ) -> Result<CallToolResult, McpError> {
        let p = match self.project(args.project.as_deref()) {
            Ok(p) => p,
            Err(e) => return tool_err_msg(e),
        };
        let state = self.storage.load_task_state(&p.name).unwrap_or_default();
        json_ok(&state.test_status())
    }

    #[tool(description = "Return the latest Executor summary.")]
    fn execution_summary(
        &self,
        Parameters(args): Parameters<ProjectArgs>,
    ) -> Result<CallToolResult, McpError> {
        let p = match self.project(args.project.as_deref()) {
            Ok(p) => p,
            Err(e) => return tool_err_msg(e),
        };
        let state = self.storage.load_task_state(&p.name).unwrap_or_default();
        json_ok(&state.execution_summary())
    }

    #[tool(description = "Start a local task.")]
    async fn task_start(
        &self,
        Parameters(args): Parameters<TaskStartArgs>,
    ) -> Result<CallToolResult, McpError> {
        let p = match self.project(args.project.as_deref()) {
            Ok(p) => p,
            Err(e) => return tool_err_msg(e),
        };
        if p.readonly {
            return tool_err_msg(format!("project `{}` is read-only", p.name));
        }
        let plan = PlanInput {
            actions: args.plan.actions,
            tests: args.plan.tests,
            success_criteria: args.plan.success_criteria,
        };
        match p
            .runtime
            .start_task(args.goal, plan, args.executor.as_deref())
            .await
        {
            Ok(state) => {
                json_ok(&serde_json::json!({ "task_id": state.task_id, "status": state.status }))
            }
            Err(e) => tool_err_msg(e.to_string()),
        }
    }

    #[tool(description = "Return task status.")]
    async fn task_status(
        &self,
        Parameters(args): Parameters<TaskIdArgs>,
    ) -> Result<CallToolResult, McpError> {
        let p = match self.project(args.project.as_deref()) {
            Ok(p) => p,
            Err(e) => return tool_err_msg(e),
        };
        match p.runtime.status(args.task_id.as_deref()).await {
            Ok(state) => json_ok(&state.task_status_payload()),
            Err(e) => tool_err_msg(e.to_string()),
        }
    }

    #[tool(description = "Cancel the running task.")]
    async fn task_cancel(
        &self,
        Parameters(args): Parameters<TaskIdArgs>,
    ) -> Result<CallToolResult, McpError> {
        let p = match self.project(args.project.as_deref()) {
            Ok(p) => p,
            Err(e) => return tool_err_msg(e),
        };
        match p.runtime.cancel(args.task_id.as_deref()).await {
            Ok(state) => json_ok(&state.task_status_payload()),
            Err(e) => tool_err_msg(e.to_string()),
        }
    }
}

#[tool_handler]
impl ServerHandler for AgentBridgeMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "agentbridge",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(INSTRUCTIONS.to_string())
    }
    async fn initialize(
        &self,
        _req: rmcp::model::InitializeRequestParams,
        _ctx: rmcp::service::RequestContext<RoleServer>,
    ) -> Result<rmcp::model::InitializeResult, McpError> {
        Ok(self.get_info())
    }
}

fn json_ok(value: &impl serde::Serialize) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::success(vec![ContentBlock::text(
        serde_json::to_string_pretty(value).unwrap(),
    )]))
}
fn tool_err(err: WorkspaceError) -> Result<CallToolResult, McpError> {
    tool_err_msg(err.to_string())
}
fn tool_err_msg(message: String) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::error(vec![ContentBlock::text(message)]))
}
