// src/models/ai.rs
//! Infrastructure and AI capability request DTOs (Executors, Proxies, Providers).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::config::ProxyKind;

use crate::models::console::{ExecutorInput, ProxyInput};

/// Request object to save or update an Executor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveExecutorRequest {
    pub id: Option<String>,
    pub name: String,
    pub kind: String,
    pub command: String,
    pub executable: Option<PathBuf>,
    pub working_directory: Option<PathBuf>,
    pub proxy_id: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Request object to save or update a Proxy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveProxyRequest {
    pub id: Option<String>,
    #[serde(default = "default_proxy_name")]
    pub name: String,
    pub kind: ProxyKind,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub is_default: bool,
    /// Optional connectivity probe target; empty uses the stable built-in default.
    #[serde(default)]
    pub test_url: Option<String>,
}

fn default_proxy_name() -> String {
    "Default Proxy".to_string()
}

/// Request object to test proxy connectivity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestProxyRequest {
    pub kind: ProxyKind,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    #[serde(default)]
    pub test_url: Option<String>,
}

impl From<SaveProxyRequest> for TestProxyRequest {
    fn from(req: SaveProxyRequest) -> Self {
        Self {
            kind: req.kind,
            host: req.host,
            port: req.port,
            username: req.username,
            password: req.password,
            test_url: req.test_url,
        }
    }
}

impl From<ExecutorInput> for SaveExecutorRequest {
    fn from(input: ExecutorInput) -> Self {
        Self {
            id: input.id,
            name: input.name,
            kind: input.kind,
            command: input.command,
            executable: input
                .executable
                .filter(|v| !v.trim().is_empty())
                .map(PathBuf::from),
            working_directory: input
                .working_directory
                .filter(|v| !v.trim().is_empty())
                .map(PathBuf::from),
            proxy_id: input.proxy_id.filter(|v| !v.trim().is_empty()),
            enabled: input.enabled,
        }
    }
}

impl From<ProxyInput> for SaveProxyRequest {
    fn from(input: ProxyInput) -> Self {
        Self {
            id: Some("default".into()),
            name: "Default Proxy".into(),
            kind: input.kind,
            host: input.host.trim().to_string(),
            port: input.port,
            username: input.username,
            password: input.password,
            enabled: input.enabled,
            is_default: true,
            test_url: None,
        }
    }
}

impl From<ProxyInput> for TestProxyRequest {
    fn from(input: ProxyInput) -> Self {
        Self {
            kind: input.kind,
            host: input.host,
            port: input.port,
            username: input.username,
            password: input.password,
            test_url: None,
        }
    }
}
