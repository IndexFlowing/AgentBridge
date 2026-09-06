use std::fmt;
use std::sync::{Arc, Mutex};

use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router, ErrorData as McpError, RoleServer, ServerHandler,
};
use serde::Deserialize;
use serde_json::Value;

use crate::config::Config;
use crate::git;
use crate::projects::{ProjectHandle, ProjectHub};
use crate::state::BridgeState;
use crate::task::PlanInput;
use crate::workspace::{default_search_limit, Workspace, WorkspaceError};

const INSTRUCTIONS: &str = "\
You are the Brain. AgentBridge gives you MCP access to one or more local coding
workspaces (projects).

Inspect with the read-only tools. You must NOT write files, delete files, run
shell commands, commit, or push. You never pass an executable or a shell
command to any tool.

Call list_projects first when multiple workspaces are mounted. switch_project
changes this session's default project. Most tools accept an optional `project`
parameter to target a workspace without switching.

When implementation is required:
1. Inspect with workspace_info, list_directory, search_workspace, read_file.
2. Produce a compact C2C PLAN (GOAL, ACTIONS, TESTS, SUCCESS_CRITERIA). Do not paste source.
3. Call task_start.
4. Poll task_status until the Executor is no longer running.
5. Inspect git_diff, test_status, and execution_summary.
6. Review. Then DONE, a new PLAN (task_start again), or BLOCKED.

The Executor (OpenCode) is the only component that edits files and runs tests.
Paths cannot escape the selected project's root.
";

#[derive(Clone)]
pub struct AgentBridgeMcp {
    hub: Arc<ProjectHub>,
    config: Arc<Config>,
    active_project: Arc<Mutex<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PathArgs {
    /// Workspace-relative path. Use "." for the workspace root.
    #[serde(default = "default_dot")]
    pub path: String,
    /// Optional project name. Defaults to the session's active project.
    #[serde(default)]
    pub project: Option<String>,
}

fn default_dot() -> String {
    ".".to_string()
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReadFileArgs {
    /// Workspace-relative path to a UTF-8 text file.
    pub path: String,
    /// Optional project name. Defaults to the session's active project.
    #[serde(default)]
    pub project: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchArgs {
    /// Text to search for in workspace files.
    pub query: String,
    /// Optional project name. Defaults to the session's active project.
    #[serde(default)]
    pub project: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GitDiffArgs {
    /// If true, return the staged (index) diff. Defaults to the working tree diff.
    #[serde(default)]
    pub staged: bool,
    /// Optional project name. Defaults to the session's active project.
    #[serde(default)]
    pub project: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ProjectArgs {
    /// Optional project name. Defaults to the session's active project.
    #[serde(default)]
    pub project: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SwitchProjectArgs {
    /// Name of the project to make active for this MCP session.
    pub project_name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TaskStartArgs {
    /// High-level goal for this iteration. Do not paste source code.
    pub goal: String,
    pub plan: PlanArgs,
    /// Optional project name. Defaults to the session's active project.
    #[serde(default)]
    pub project: Option<String>,
    /// Optional executor id or kind to override the project's default executor.
    #[serde(default)]
    pub executor: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PlanArgs {
    /// Concrete implementation steps. No source dumps.
    pub actions: Vec<String>,
    /// Test commands the Executor should run (e.g. ["cargo test"]).
    #[serde(default, deserialize_with = "string_or_vec")]
    #[schemars(with = "Vec<String>")]
    pub tests: Vec<String>,
    /// How the Brain will judge the review.
    pub success_criteria: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TaskIdArgs {
    /// Task id returned by task_start. Omit to use the current task.
    #[serde(default)]
    pub task_id: Option<String>,
    /// Optional project name. Defaults to the session's active project.
    #[serde(default)]
    pub project: Option<String>,
}

fn string_or_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct StringOrVec;

    impl<'de> serde::de::Visitor<'de> for StringOrVec {
        type Value = Vec<String>;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a string or an array of strings")
        }

        fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
            if v.trim().is_empty() {
                Ok(Vec::new())
            } else {
                Ok(vec![v.to_string()])
            }
        }

        fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
            self.visit_str(&v)
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            let mut out = Vec::new();
            while let Some(s) = seq.next_element::<String>()? {
                if !s.trim().is_empty() {
                    out.push(s);
                }
            }
            Ok(out)
        }

        fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(Vec::new())
        }

        fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(Vec::new())
        }
    }

    deserializer.deserialize_any(StringOrVec)
}

impl AgentBridgeMcp {
    pub fn new(hub: Arc<ProjectHub>, config: Arc<Config>) -> Self {
        let active = hub.default_name().to_string();
        Self {
            hub,
            config,
            active_project: Arc::new(Mutex::new(active)),
        }
    }

    fn active_name(&self) -> String {
        self.active_project
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn project(&self, requested: Option<&str>) -> Result<&ProjectHandle, String> {
        let name = match requested.map(str::trim).filter(|s| !s.is_empty()) {
            Some(n) => n.to_string(),
            None => self.active_name(),
        };
        self.hub.get(&name).ok_or_else(|| {
            format!(
                "unknown project `{name}`. Call list_projects to see mounted workspaces."
            )
        })
    }
}

#[tool_router]
impl AgentBridgeMcp {
    #[tool(
        description = "List mounted projects (name, path, description, readonly, active). Call this first when more than one workspace is available."
    )]
    fn list_projects(&self) -> Result<CallToolResult, McpError> {
        let active = self.active_name();
        json_ok(&serde_json::json!({
            "projects": self.hub.list(&active),
            "active_project": active,
        }))
    }

    #[tool(
        description = "Switch this MCP session's active project. Subsequent tools without a `project` argument use this workspace. Validates the name against configured projects."
    )]
    fn switch_project(
        &self,
        Parameters(args): Parameters<SwitchProjectArgs>,
    ) -> Result<CallToolResult, McpError> {
        let Some(handle) = self.hub.get(&args.project_name) else {
            return tool_err_msg(format!(
                "unknown project `{}`. Call list_projects to see mounted workspaces.",
                args.project_name
            ));
        };
        *self
            .active_project
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = handle.name.clone();
        let info = handle.workspace.info();
        json_ok(&serde_json::json!({
            "active_project": handle.name,
            "path": info.workspace,
            "description": handle.description,
            "readonly": handle.readonly,
            "project_type": info.project_type,
            "git_repository": info.git_repository,
        }))
    }

    #[tool(
        description = "Return workspace path, detected project types, and whether git is present. Optional `project` selects a mounted workspace. Read-only."
    )]
    fn workspace_info(
        &self,
        Parameters(args): Parameters<ProjectArgs>,
    ) -> Result<CallToolResult, McpError> {
        match self.project(args.project.as_deref()) {
            Ok(p) => {
                let info = p.workspace.info();
                json_ok(&serde_json::json!({
                    "project": p.name,
                    "description": p.description,
                    "readonly": p.readonly,
                    "workspace": info.workspace,
                    "project_type": info.project_type,
                    "git_repository": info.git_repository,
                    "active": p.name == self.active_name(),
                }))
            }
            Err(err) => tool_err_msg(err),
        }
    }

    #[tool(
        description = "List a directory inside the selected project. Paths cannot escape that project's root. Optional `project` overrides the active workspace. Read-only."
    )]
    fn list_directory(
        &self,
        Parameters(args): Parameters<PathArgs>,
    ) -> Result<CallToolResult, McpError> {
        let p = match self.project(args.project.as_deref()) {
            Ok(p) => p,
            Err(err) => return tool_err_msg(err),
        };
        match p.workspace.list_directory(&args.path) {
            Ok(listing) => json_ok(&serde_json::json!({
                "project": p.name,
                "path": listing.path,
                "entries": listing.entries,
            })),
            Err(err) => tool_err(err),
        }
    }

    #[tool(
        description = "Read a UTF-8 text file inside the selected project. Rejects path traversal, secrets, binaries, and oversized files. Optional `project` overrides the active workspace. Read-only."
    )]
    fn read_file(
        &self,
        Parameters(args): Parameters<ReadFileArgs>,
    ) -> Result<CallToolResult, McpError> {
        let p = match self.project(args.project.as_deref()) {
            Ok(p) => p,
            Err(err) => return tool_err_msg(err),
        };
        match p.workspace.read_file(&args.path) {
            Ok(text) => json_ok(&serde_json::json!({
                "project": p.name,
                "path": args.path,
                "content": text,
            })),
            Err(err) => tool_err(err),
        }
    }

    #[tool(
        description = "Search UTF-8 text files in the selected project. Skips .git, node_modules, target, dist, build, and .cache. Optional `project` overrides the active workspace. Read-only."
    )]
    fn search_workspace(
        &self,
        Parameters(args): Parameters<SearchArgs>,
    ) -> Result<CallToolResult, McpError> {
        let p = match self.project(args.project.as_deref()) {
            Ok(p) => p,
            Err(err) => return tool_err_msg(err),
        };
        match p.workspace.search(&args.query, default_search_limit()) {
            Ok(results) => json_ok(&serde_json::json!({
                "project": p.name,
                "query": results.query,
                "hits": results.hits,
                "truncated": results.truncated,
            })),
            Err(err) => tool_err(err),
        }
    }

    #[tool(
        description = "Return git branch, clean/dirty state, and changed/staged/untracked files for the selected project. Uses the system git binary. Read-only."
    )]
    fn git_status(
        &self,
        Parameters(args): Parameters<ProjectArgs>,
    ) -> Result<CallToolResult, McpError> {
        let p = match self.project(args.project.as_deref()) {
            Ok(p) => p,
            Err(err) => return tool_err_msg(err),
        };
        match git::status(p.workspace.root()) {
            Ok(status) => json_ok(&serde_json::json!({
                "project": p.name,
                "branch": status.branch,
                "clean": status.clean,
                "changed_files": status.changed_files,
                "staged_files": status.staged_files,
                "untracked_files": status.untracked_files,
            })),
            Err(err) => tool_err_msg(err.to_string()),
        }
    }

    #[tool(
        description = "Return the current git diff for the selected project. Default is the working tree; set staged=true for the index. Large diffs are truncated. Read-only."
    )]
    fn git_diff(
        &self,
        Parameters(args): Parameters<GitDiffArgs>,
    ) -> Result<CallToolResult, McpError> {
        let p = match self.project(args.project.as_deref()) {
            Ok(p) => p,
            Err(err) => return tool_err_msg(err),
        };
        match git::diff(
            p.workspace.root(),
            args.staged,
            self.config.security.max_diff_bytes,
        ) {
            Ok(diff) => json_ok(&serde_json::json!({
                "project": p.name,
                "staged": diff.staged,
                "diff": diff.diff,
                "truncated": diff.truncated,
                "original_bytes": diff.original_bytes,
            })),
            Err(err) => tool_err_msg(err.to_string()),
        }
    }

    #[tool(
        description = "Return the latest test result recorded by the Executor for the selected project. This tool does not run tests."
    )]
    fn test_status(
        &self,
        Parameters(args): Parameters<ProjectArgs>,
    ) -> Result<CallToolResult, McpError> {
        let p = match self.project(args.project.as_deref()) {
            Ok(p) => p,
            Err(err) => return tool_err_msg(err),
        };
        let state = BridgeState::load(p.workspace.root()).map_err(internal)?;
        json_ok(&state.test_status())
    }

    #[tool(
        description = "Return the latest Executor result for the selected project: task id, iteration, status, changed files, and tests. Read-only."
    )]
    fn execution_summary(
        &self,
        Parameters(args): Parameters<ProjectArgs>,
    ) -> Result<CallToolResult, McpError> {
        let p = match self.project(args.project.as_deref()) {
            Ok(p) => p,
            Err(err) => return tool_err_msg(err),
        };
        let state = BridgeState::load(p.workspace.root()).map_err(internal)?;
        json_ok(&state.execution_summary())
    }

    #[tool(
        description = "Create a task from a compact C2C PLAN and start the local executor in the selected project. Returns immediately with task_id; poll task_status until completion. Rejected on readonly projects."
    )]
    async fn task_start(
        &self,
        Parameters(args): Parameters<TaskStartArgs>,
    ) -> Result<CallToolResult, McpError> {
        let p = match self.project(args.project.as_deref()) {
            Ok(p) => p,
            Err(err) => return tool_err_msg(err),
        };
        if p.readonly {
            return tool_err_msg(format!(
                "project `{}` is read-only; task_start is not allowed",
                p.name
            ));
        }
        let plan = PlanInput {
            actions: args.plan.actions,
            tests: args.plan.tests,
            success_criteria: args.plan.success_criteria,
        };
        // 👈 传递 args.executor.as_deref()
        match p.runtime.start_task(args.goal, plan, args.executor.as_deref()).await {
            Ok(state) => json_ok(&serde_json::json!({
                "project": p.name,
                "task_id": state.task_id,
                "iteration": state.iteration,
                "status": state.status,
                "lifecycle": state.task_status.map(|s| s.as_str()),
                "executor": state.executor,
            })),
            Err(err) => tool_err_msg(err.to_string()),
        }
    }

    #[tool(
        description = "Return the current or specified task status: running | success | failed | blocked | cancelled, plus summary, exit_code, tests, and changed_files. Does not run the executor."
    )]
    async fn task_status(
        &self,
        Parameters(args): Parameters<TaskIdArgs>,
    ) -> Result<CallToolResult, McpError> {
        let p = match self.project(args.project.as_deref()) {
            Ok(p) => p,
            Err(err) => return tool_err_msg(err),
        };
        match p.runtime.status(args.task_id.as_deref()).await {
            Ok(state) => json_ok(&state.task_status_payload()),
            Err(err) => tool_err_msg(err.to_string()),
        }
    }

    #[tool(
        description = "Cancel the running OpenCode executor for the current or specified task. Safe process-tree termination. Does not accept a shell command."
    )]
    async fn task_cancel(
        &self,
        Parameters(args): Parameters<TaskIdArgs>,
    ) -> Result<CallToolResult, McpError> {
        let p = match self.project(args.project.as_deref()) {
            Ok(p) => p,
            Err(err) => return tool_err_msg(err),
        };
        match p.runtime.cancel(args.task_id.as_deref()).await {
            Ok(state) => json_ok(&state.task_status_payload()),
            Err(err) => tool_err_msg(err.to_string()),
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
        _request: rmcp::model::InitializeRequestParams,
        _context: rmcp::service::RequestContext<RoleServer>,
    ) -> Result<rmcp::model::InitializeResult, McpError> {
        Ok(self.get_info())
    }
}

fn json_ok(value: &impl serde::Serialize) -> Result<CallToolResult, McpError> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
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

/// Used by tests to call workspace operations without MCP plumbing.
pub fn eval_tool(workspace: &Workspace, name: &str, args: Value) -> Result<Value, String> {
    match name {
        "workspace_info" => serde_json::to_value(workspace.info()).map_err(|e| e.to_string()),
        "list_directory" => {
            let path = args.get("path").and_then(Value::as_str).unwrap_or(".");
            workspace
                .list_directory(path)
                .map_err(|e| e.to_string())
                .and_then(|v| serde_json::to_value(v).map_err(|e| e.to_string()))
        }
        "read_file" => {
            let path = args
                .get("path")
                .and_then(Value::as_str)
                .ok_or("missing path")?;
            workspace
                .read_file(path)
                .map(|content| serde_json::json!({ "path": path, "content": content }))
                .map_err(|e| e.to_string())
        }
        "search_workspace" => {
            let query = args
                .get("query")
                .and_then(Value::as_str)
                .ok_or("missing query")?;
            workspace
                .search(query, default_search_limit())
                .map_err(|e| e.to_string())
                .and_then(|v| serde_json::to_value(v).map_err(|e| e.to_string()))
        }
        _ => Err(format!("unknown tool {name}")),
    }
}
