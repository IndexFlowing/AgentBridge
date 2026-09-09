pub mod schema;

use std::sync::{Arc, Mutex};
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router, ErrorData as McpError, RoleServer, ServerHandler,
};
use serde_json::Value;

use crate::config::Config;
use crate::git;
use crate::mcp::schema::*;
use crate::projects::{ProjectHandle, ProjectHub};
use crate::state::BridgeState;
use crate::task::PlanInput;
use crate::workspace::{default_search_limit, Workspace, WorkspaceError};

const INSTRUCTIONS: &str = "\
You are the Brain. AgentBridge gives you MCP access to local coding workspaces.
Inspect with read-only tools. The Executor (OpenCode) is the only component that edits files and runs tests.
";

#[derive(Clone)]
pub struct AgentBridgeMcp {
    pub hub: Arc<ProjectHub>,
    pub config: Arc<Config>,
    pub active_project: Arc<Mutex<String>>,
}

impl AgentBridgeMcp {
    pub fn new(hub: Arc<ProjectHub>, config: Arc<Config>) -> Self {
        let active = hub.default_name();
        Self {
            hub,
            config,
            active_project: Arc::new(Mutex::new(active)),
        }
    }

    pub fn active_name(&self) -> String {
        self.active_project.lock().unwrap().clone()
    }

    pub fn project(&self, requested: Option<&str>) -> Result<ProjectHandle, String> {
        let name = match requested.map(str::trim).filter(|s| !s.is_empty()) {
            Some(n) => n.to_string(),
            None => self.active_name(),
        };
        self.hub.get(&name).ok_or_else(|| format!("unknown project `{name}`"))
    }
}

#[tool_router]
impl AgentBridgeMcp {
    #[tool(description = "List mounted projects (name, path, description, readonly, active).")]
    fn list_projects(&self) -> Result<CallToolResult, McpError> {
        let active = self.active_name();
        let list = self.hub.list(&active);
        eprintln!("[MCP-Tool] list_projects => 返回 {} 个工作区 (当前活跃: '{}')", list.len(), active);
        json_ok(&serde_json::json!({ "projects": list, "active_project": active }))
    }

    #[tool(description = "Switch active project for this session.")]
    fn switch_project(&self, Parameters(args): Parameters<SwitchProjectArgs>) -> Result<CallToolResult, McpError> {
        eprintln!("[MCP-Tool] switch_project => 切换到项目: '{}'", args.project_name);
        let Some(handle) = self.hub.get(&args.project_name) else {
            return tool_err_msg(format!("unknown project `{}`", args.project_name));
        };
        *self.active_project.lock().unwrap() = handle.name.clone();
        let info = handle.workspace.info();
        json_ok(&serde_json::json!({
            "active_project": handle.name, "path": info.workspace, "description": handle.description,
            "readonly": handle.readonly, "project_type": info.project_type, "git_repository": info.git_repository,
        }))
    }

    #[tool(description = "Return workspace path and detected project types.")]
    fn workspace_info(&self, Parameters(args): Parameters<ProjectArgs>) -> Result<CallToolResult, McpError> {
        let p = match self.project(args.project.as_deref()) { Ok(p) => p, Err(e) => return tool_err_msg(e) };
        let info = p.workspace.info();
        eprintln!("[MCP-Tool] workspace_info => 项目: '{}', 路径: '{}'", p.name, info.workspace);
        json_ok(&serde_json::json!({
            "project": p.name, "description": p.description, "readonly": p.readonly, "workspace": info.workspace,
            "project_type": info.project_type, "git_repository": info.git_repository, "active": p.name == self.active_name(),
        }))
    }

    #[tool(description = "List a directory inside the selected project.")]
    fn list_directory(&self, Parameters(args): Parameters<PathArgs>) -> Result<CallToolResult, McpError> {
        let p = match self.project(args.project.as_deref()) { Ok(p) => p, Err(e) => return tool_err_msg(e) };
        eprintln!("[MCP-Tool] list_directory => 项目: '{}', 路径: '{}'", p.name, args.path);
        match p.workspace.list_directory(&args.path) {
            Ok(listing) => json_ok(&serde_json::json!({ "project": p.name, "path": listing.path, "entries": listing.entries })),
            Err(e) => tool_err(e),
        }
    }

    #[tool(description = "Read a UTF-8 text file inside the selected project.")]
    fn read_file(&self, Parameters(args): Parameters<ReadFileArgs>) -> Result<CallToolResult, McpError> {
        let p = match self.project(args.project.as_deref()) { Ok(p) => p, Err(e) => return tool_err_msg(e) };
        eprintln!("[MCP-Tool] read_file => 项目: '{}', 文件: '{}'", p.name, args.path);
        match p.workspace.read_file(&args.path) {
            Ok(content) => json_ok(&serde_json::json!({ "project": p.name, "path": args.path, "content": content })),
            Err(e) => tool_err(e),
        }
    }

    #[tool(description = "Search UTF-8 text files in the selected project.")]
    fn search_workspace(&self, Parameters(args): Parameters<SearchArgs>) -> Result<CallToolResult, McpError> {
        let p = match self.project(args.project.as_deref()) { Ok(p) => p, Err(e) => return tool_err_msg(e) };
        eprintln!("[MCP-Tool] search_workspace => 项目: '{}', 关键词: '{}'", p.name, args.query);
        match p.workspace.search(&args.query, default_search_limit()) {
            Ok(res) => json_ok(&serde_json::json!({ "project": p.name, "query": res.query, "hits": res.hits, "truncated": res.truncated })),
            Err(e) => tool_err(e),
        }
    }

    #[tool(description = "Return git status for the selected project.")]
    fn git_status(&self, Parameters(args): Parameters<ProjectArgs>) -> Result<CallToolResult, McpError> {
        let p = match self.project(args.project.as_deref()) { Ok(p) => p, Err(e) => return tool_err_msg(e) };
        eprintln!("[MCP-Tool] git_status => 项目: '{}'", p.name);
        match git::status(p.workspace.root()) {
            Ok(s) => json_ok(&serde_json::json!({ "project": p.name, "branch": s.branch, "clean": s.clean, "changed_files": s.changed_files, "staged_files": s.staged_files, "untracked_files": s.untracked_files })),
            Err(e) => tool_err_msg(e.to_string()),
        }
    }

    #[tool(description = "Return git diff for the selected project.")]
    fn git_diff(&self, Parameters(args): Parameters<GitDiffArgs>) -> Result<CallToolResult, McpError> {
        let p = match self.project(args.project.as_deref()) { Ok(p) => p, Err(e) => return tool_err_msg(e) };
        eprintln!("[MCP-Tool] git_diff => 项目: '{}', staged={}", p.name, args.staged);
        match git::diff(p.workspace.root(), args.staged, self.config.security.max_diff_bytes) {
            Ok(d) => json_ok(&serde_json::json!({ "project": p.name, "staged": d.staged, "diff": d.diff, "truncated": d.truncated, "original_bytes": d.original_bytes })),
            Err(e) => tool_err_msg(e.to_string()),
        }
    }

    #[tool(description = "Return the latest test result recorded by the Executor.")]
    fn test_status(&self, Parameters(args): Parameters<ProjectArgs>) -> Result<CallToolResult, McpError> {
        let p = match self.project(args.project.as_deref()) { Ok(p) => p, Err(e) => return tool_err_msg(e) };
        eprintln!("[MCP-Tool] test_status => 项目: '{}'", p.name);
        let state = BridgeState::load(p.workspace.root()).map_err(internal)?;
        json_ok(&state.test_status())
    }

    #[tool(description = "Return the latest Executor result: task id, status, changed files.")]
    fn execution_summary(&self, Parameters(args): Parameters<ProjectArgs>) -> Result<CallToolResult, McpError> {
        let p = match self.project(args.project.as_deref()) { Ok(p) => p, Err(e) => return tool_err_msg(e) };
        eprintln!("[MCP-Tool] execution_summary => 项目: '{}'", p.name);
        let state = BridgeState::load(p.workspace.root()).map_err(internal)?;
        json_ok(&state.execution_summary())
    }

    #[tool(description = "Create a task from a C2C PLAN and start the local executor.")]
    async fn task_start(&self, Parameters(args): Parameters<TaskStartArgs>) -> Result<CallToolResult, McpError> {
        let p = match self.project(args.project.as_deref()) { Ok(p) => p, Err(e) => return tool_err_msg(e) };
        eprintln!("[MCP-Tool] task_start => 目标: '{}', Executor: {:?}", args.goal, args.executor);
        if p.readonly { return tool_err_msg(format!("project `{}` is read-only", p.name)); }
        let plan = PlanInput { actions: args.plan.actions, tests: args.plan.tests, success_criteria: args.plan.success_criteria };
        match p.runtime.start_task(args.goal, plan, args.executor.as_deref()).await {
            Ok(state) => json_ok(&serde_json::json!({ "project": p.name, "task_id": state.task_id, "iteration": state.iteration, "status": state.status, "executor": state.executor })),
            Err(e) => { eprintln!("[MCP-Tool] task_start 失败: {}", e); tool_err_msg(e.to_string()) },
        }
    }

    #[tool(description = "Return task status.")]
    async fn task_status(&self, Parameters(args): Parameters<TaskIdArgs>) -> Result<CallToolResult, McpError> {
        let p = match self.project(args.project.as_deref()) { Ok(p) => p, Err(e) => return tool_err_msg(e) };
        eprintln!("[MCP-Tool] task_status => 项目: '{}', TaskID: {:?}", p.name, args.task_id);
        match p.runtime.status(args.task_id.as_deref()).await {
            Ok(state) => json_ok(&state.task_status_payload()),
            Err(e) => tool_err_msg(e.to_string()),
        }
    }

    #[tool(description = "Cancel the running executor task.")]
    async fn task_cancel(&self, Parameters(args): Parameters<TaskIdArgs>) -> Result<CallToolResult, McpError> {
        let p = match self.project(args.project.as_deref()) { Ok(p) => p, Err(e) => return tool_err_msg(e) };
        eprintln!("[MCP-Tool] task_cancel => 取消任务: {:?}", args.task_id);
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
            .with_server_info(Implementation::new("agentbridge", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS.to_string())
    }

    async fn initialize(&self, _req: rmcp::model::InitializeRequestParams, _ctx: rmcp::service::RequestContext<RoleServer>) -> Result<rmcp::model::InitializeResult, McpError> {
        eprintln!("[MCP] 收到客户端握手请求 (initialize)");
        Ok(self.get_info())
    }
}

fn json_ok(value: &impl serde::Serialize) -> Result<CallToolResult, McpError> {
    let text = serde_json::to_string_pretty(value).map_err(|e| McpError::internal_error(e.to_string(), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
}

fn tool_err(err: WorkspaceError) -> Result<CallToolResult, McpError> {
    tool_err_msg(err.to_string())
}

fn tool_err_msg(message: String) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::error(vec![ContentBlock::text(message)]))
}

fn internal(err: impl std::fmt::Display) -> McpError {
    McpError::internal_error(err.to_string(), None)
}

pub fn eval_tool(workspace: &Workspace, name: &str, args: Value) -> Result<Value, String> {
    match name {
        "workspace_info" => serde_json::to_value(workspace.info()).map_err(|e| e.to_string()),
        "list_directory" => {
            let path = args.get("path").and_then(Value::as_str).unwrap_or(".");
            workspace.list_directory(path).map_err(|e| e.to_string()).and_then(|v| serde_json::to_value(v).map_err(|e| e.to_string()))
        }
        "read_file" => {
            let path = args.get("path").and_then(Value::as_str).ok_or("missing path")?;
            workspace.read_file(path).map(|c| serde_json::json!({ "path": path, "content": c })).map_err(|e| e.to_string())
        }
        "search_workspace" => {
            let query = args.get("query").and_then(Value::as_str).ok_or("missing query")?;
            workspace.search(query, default_search_limit()).map_err(|e| e.to_string()).and_then(|v| serde_json::to_value(v).map_err(|e| e.to_string()))
        }
        _ => Err(format!("unknown tool {name}")),
    }
}