//! AgentBridge: MCP access to a local coding workspace.
//!
//! The Brain (a remote AI) inspects the workspace through read-only MCP tools
//! and starts a local Executor (OpenCode) with a structured C2C PLAN. The
//! Executor is the only component that writes files or runs commands.

pub mod config;
pub mod doctor;
pub mod executor;
pub mod git;
pub mod mcp;
pub mod oauth;
pub mod projects;
pub mod protocol;
pub mod server;
pub mod state;
pub mod task;
pub mod workspace;

pub use config::Config;
pub use projects::ProjectHub;
pub use protocol::{C2cMessage, C2cPlan, C2cState};
pub use task::TaskRuntime;
pub use workspace::{Workspace, WorkspaceError};
