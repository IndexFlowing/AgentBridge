//! AgentBridge: read-only MCP access to a local coding workspace.
//!
//! The Brain (a remote AI) inspects the workspace through MCP. The Executor
//! (a local coding agent) is the only component that writes files or runs
//! commands. This crate implements the local bridge.

pub mod config;
pub mod doctor;
pub mod executor;
pub mod git;
pub mod mcp;
pub mod protocol;
pub mod server;
pub mod state;
pub mod workspace;

pub use config::Config;
pub use protocol::{C2cMessage, C2cState};
pub use workspace::{Workspace, WorkspaceError};
