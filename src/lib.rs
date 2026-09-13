// src/lib.rs
//! AgentBridge: MCP access to a local coding workspace.
//!
//! The Brain (a remote AI) inspects the workspace through read-only MCP tools
//! and starts a local Executor (OpenCode) with a structured C2C PLAN. The
//! Executor is the only component that writes files or runs commands.

// === 我们新增的三个核心现代化模块 ===
pub mod api;
pub mod models;
pub mod storage;

// === 原有的模块 ===
pub mod config;
pub mod dashboard;
pub mod doctor;
pub mod executor;
pub mod git;
pub mod mcp;
pub mod oauth;
pub mod projects;
pub mod protocol;
pub mod server;
pub mod service;
pub mod state;
pub mod task;
pub mod tunnel;
pub mod workspace;

// 暴露常用的类型供其他地方使用
pub use config::Config;
pub use projects::ProjectHub;
pub use protocol::{C2cMessage, C2cPlan, C2cState};
pub use task::TaskRuntime;
pub use workspace::{Workspace, WorkspaceError};
