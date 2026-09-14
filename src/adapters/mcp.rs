// src/mcp/mod.rs
pub mod schema;

use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerInfo},
    ErrorData as McpError, RoleServer, ServerHandler,
};
use rmcp::{tool, tool_handler, tool_router};
use std::sync::{Arc, Mutex};

use crate::git;
use crate::mcp::schema::*;
use crate::models::{CancelTaskRequest, StartTaskRequest};
use crate::projects::ProjectHandle;

use crate::task::PlanInput;
use crate::workspace::{default_search_limit, WorkspaceError};

const INSTRUCTIONS: &str =
    "You are the Brain. Inspect with read-only tools. The Executor (OpenCode) edits files.";

use crate::core::AppCore;
use std::ops::Deref;

#[derive(Clone)]
pub struct AgentBridgeMcp {
    pub core: Arc<AppCore>,
    pub active_project: Arc<Mutex<String>>,
}

impl Deref for AgentBridgeMcp {
    type Target = AppCore;
    fn deref(&self) -> &Self::Target {
        &self.core
    }
}

impl AgentBridgeMcp {
    pub fn new(core: Arc<AppCore>) -> Self {
        let active = core.hub.default_name();
        Self {
            core,
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
        match self.projects.list_active(self.active_name()) {
            Ok(projects) => json_ok(&serde_json::json!({
                "projects": projects,
                "active_project": self.active_name()
            })),
            Err(e) => tool_err_msg(e.to_string()),
        }
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
        let state = self.tasks.task_state(&p.name).unwrap_or_default();
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
        let state = self.tasks.task_state(&p.name).unwrap_or_default();
        json_ok(&state.execution_summary())
    }

    #[tool(description = "Start a local task.")]
    async fn task_start(
        &self,
        Parameters(args): Parameters<TaskStartArgs>,
    ) -> Result<CallToolResult, McpError> {
        let project_name = args.project.unwrap_or_else(|| self.active_name());
        let req = StartTaskRequest {
            project_name,
            goal: args.goal,
            plan: PlanInput {
                actions: args.plan.actions,
                tests: args.plan.tests,
                success_criteria: args.plan.success_criteria,
            },
            skills: args.skills, // <--- 传入 Brain 选定的 skills 列表
            executor: args.executor,
            continue_task_id: args.continue_task_id,
        };
        match self.tasks.start_task(req).await {
            Ok(state) => json_ok(&serde_json::json!({
                "task_id": state.task_id,
                "iteration": state.iteration,
                "status": state.status
            })),
            Err(e) => tool_err_msg(e.to_string()),
        }
    }

    #[tool(description = "Return task status.")]
    async fn task_status(
        &self,
        Parameters(args): Parameters<TaskIdArgs>,
    ) -> Result<CallToolResult, McpError> {
        let project_name = args.project.unwrap_or_else(|| self.active_name());
        match self
            .tasks
            .get_status(&project_name, args.task_id.as_deref())
            .await
        {
            Ok(state) => json_ok(&state.task_status_payload()),
            Err(e) => tool_err_msg(e.to_string()),
        }
    }

    #[tool(description = "Cancel the running task.")]
    async fn task_cancel(
        &self,
        Parameters(args): Parameters<TaskIdArgs>,
    ) -> Result<CallToolResult, McpError> {
        let project_name = args.project.unwrap_or_else(|| self.active_name());
        let req = CancelTaskRequest {
            project_name,
            task_id: args.task_id,
        };
        match self.tasks.cancel_task(req).await {
            Ok(state) => json_ok(&state.task_status_payload()),
            Err(e) => tool_err_msg(e.to_string()),
        }
    }

    #[tool(description = "List all available skills enabled for the current active project.")]
    fn list_skills(&self) -> Result<CallToolResult, McpError> {
        let candidates = self
            .skills
            .resolve_candidates(Some(&self.active_name()), "");
        json_ok(&candidates)
    }

    #[tool(
        description = "Read detailed SKILL.md documentation and guidelines for a specific skill."
    )]
    fn read_skill(
        &self,
        Parameters(args): Parameters<SkillReadArgs>,
    ) -> Result<CallToolResult, McpError> {
        match self.skills.get_skill_detail(&args.skill_name) {
            Ok(d) => json_ok(&d),
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
