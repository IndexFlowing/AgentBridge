// src/lib.rs
//! AgentBridge: MCP access to a local coding workspace.

pub mod adapters;
pub mod core;
pub mod infra;
pub mod models;

// 顶级统一重导出（让内部 crate::xxx 和外部测试无缝使用）：
pub use adapters::api;
pub use adapters::cli;
pub use adapters::dashboard;
pub use adapters::doctor;
pub use adapters::mcp;
pub use adapters::oauth;
pub use adapters::server;
pub use adapters::server::tunnel;

pub use core::executor;
pub use core::projects;
pub use core::protocol;
pub use core::provider;
pub use core::provider::credentials;
pub use core::state;
pub use core::task;
pub use core::workspace;
pub use core::workspace::git;
pub use core::skill;

pub use infra::config;
pub use infra::daemon;
pub use infra::daemon as service; // 兼容历史别名
pub use infra::storage;

// 常用根类型重导出
pub use config::Config;
pub use projects::ProjectHub;
pub use protocol::{C2cMessage, C2cPlan, C2cState};
pub use task::TaskRuntime;
pub use workspace::{Workspace, WorkspaceError};