use std::sync::Arc;

use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router, ErrorData as McpError, RoleServer, ServerHandler,
};
use serde::Deserialize;
use serde_json::Value;

use crate::config::Config;
use crate::git;
use crate::state::BridgeState;
use crate::workspace::{default_search_limit, Workspace, WorkspaceError};

const INSTRUCTIONS: &str = "\
You are the Brain. AgentBridge gives you read-only MCP access to a local coding workspace.

You may inspect files, search, and read git/test/execution status.
You must NOT write files, delete files, run shell commands, commit, or push.

Produce compact C2C PLAN and REVIEW messages. Do not paste entire source files
or huge diffs into the conversation; use these tools instead.

The Executor (a local coding agent such as OpenCode) performs all modifications
and test runs, then records results with `agentbridge task executed`.
";

#[derive(Clone)]
pub struct AgentBridgeMcp {
    workspace: Arc<Workspace>,
    config: Arc<Config>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PathArgs {
    /// Workspace-relative path. Use "." for the workspace root.
    #[serde(default = "default_dot")]
    pub path: String,
}

fn default_dot() -> String {
    ".".to_string()
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReadFileArgs {
    /// Workspace-relative path to a UTF-8 text file.
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchArgs {
    /// Text to search for in workspace files.
    pub query: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GitDiffArgs {
    /// If true, return the staged (index) diff. Defaults to the working tree diff.
    #[serde(default)]
    pub staged: bool,
}

impl AgentBridgeMcp {
    pub fn new(workspace: Arc<Workspace>, config: Arc<Config>) -> Self {
        Self { workspace, config }
    }
}

#[tool_router]
impl AgentBridgeMcp {
    #[tool(
        description = "Return workspace path, detected project types, and whether git is present. Read-only."
    )]
    fn workspace_info(&self) -> Result<CallToolResult, McpError> {
        json_ok(&self.workspace.info())
    }

    #[tool(
        description = "List a directory inside the workspace. Paths cannot escape the workspace. Read-only."
    )]
    fn list_directory(
        &self,
        Parameters(args): Parameters<PathArgs>,
    ) -> Result<CallToolResult, McpError> {
        match self.workspace.list_directory(&args.path) {
            Ok(listing) => json_ok(&listing),
            Err(err) => tool_err(err),
        }
    }

    #[tool(
        description = "Read a UTF-8 text file inside the workspace. Rejects path traversal, secrets, binaries, and oversized files. Read-only."
    )]
    fn read_file(
        &self,
        Parameters(args): Parameters<ReadFileArgs>,
    ) -> Result<CallToolResult, McpError> {
        match self.workspace.read_file(&args.path) {
            Ok(text) => json_ok(&serde_json::json!({
                "path": args.path,
                "content": text,
            })),
            Err(err) => tool_err(err),
        }
    }

    #[tool(
        description = "Search UTF-8 text files in the workspace. Skips .git, node_modules, target, dist, build, and .cache. Read-only."
    )]
    fn search_workspace(
        &self,
        Parameters(args): Parameters<SearchArgs>,
    ) -> Result<CallToolResult, McpError> {
        match self.workspace.search(&args.query, default_search_limit()) {
            Ok(results) => json_ok(&results),
            Err(err) => tool_err(err),
        }
    }

    #[tool(
        description = "Return git branch, clean/dirty state, and changed/staged/untracked files. Uses the system git binary. Read-only."
    )]
    fn git_status(&self) -> Result<CallToolResult, McpError> {
        match git::status(self.workspace.root()) {
            Ok(status) => json_ok(&status),
            Err(err) => tool_err_msg(err.to_string()),
        }
    }

    #[tool(
        description = "Return the current git diff. Default is the working tree; set staged=true for the index. Large diffs are truncated. Read-only."
    )]
    fn git_diff(
        &self,
        Parameters(args): Parameters<GitDiffArgs>,
    ) -> Result<CallToolResult, McpError> {
        match git::diff(
            self.workspace.root(),
            args.staged,
            self.config.security.max_diff_bytes,
        ) {
            Ok(diff) => json_ok(&diff),
            Err(err) => tool_err_msg(err.to_string()),
        }
    }

    #[tool(
        description = "Return the latest test result recorded by the Executor. This tool does not run tests."
    )]
    fn test_status(&self) -> Result<CallToolResult, McpError> {
        let state = BridgeState::load(self.workspace.root()).map_err(internal)?;
        json_ok(&state.test_status())
    }

    #[tool(
        description = "Return the latest Executor result: task id, iteration, status, changed files, and tests. Read-only."
    )]
    fn execution_summary(&self) -> Result<CallToolResult, McpError> {
        let state = BridgeState::load(self.workspace.root()).map_err(internal)?;
        json_ok(&state.execution_summary())
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
