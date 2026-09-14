// src/infra/config.rs
//! Startup configuration, user paths, proxy specs, and ProxyService.

pub mod patch;
pub mod paths;
pub mod proxy;
pub mod proxy_service;
pub mod types;

pub use patch::*;
pub use paths::*;
pub use proxy::*;
pub use proxy_service::*;
pub use types::*;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub workspace: PathBuf,
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_true")]
    pub allow_any_host: bool,
    #[serde(default)]
    pub auth_token: Option<String>,
    #[serde(default)]
    pub admin_password: Option<String>,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub client_secret: Option<String>,
    #[serde(default)]
    pub no_auth: bool,
    #[serde(default)]
    pub tunnel_token: Option<String>,
    #[serde(default)]
    pub tunnel_hostname: Option<String>,
    #[serde(default)]
    pub executor: ExecutorConfig,
    #[serde(default)]
    pub proxy: ProxyConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
}

impl Config {
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            host: default_host(),
            port: default_port(),
            allow_any_host: true,
            auth_token: Some(String::new()),
            admin_password: Some(String::new()),
            client_id: Some(String::new()),
            client_secret: Some(String::new()),
            no_auth: false,
            tunnel_token: Some(String::new()),
            tunnel_hostname: Some(String::new()),
            executor: ExecutorConfig::default(),
            proxy: ProxyConfig::default(),
            security: SecurityConfig::default(),
            logging: LoggingConfig::default(),
        }
    }

    pub fn listen_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub fn mcp_url(&self) -> String {
        format!("http://{}:{}/mcp", self.host, self.port)
    }

    pub fn is_loopback(&self) -> bool {
        self.host
            .parse::<IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or_else(|_| matches!(self.host.as_str(), "localhost" | "127.0.0.1" | "::1"))
    }

    pub fn load_from_path(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        let mut cfg: Config = toml::from_str(&text)
            .with_context(|| format!("failed to parse config {}", path.display()))?;
        if cfg.workspace.as_os_str().is_empty() {
            cfg.workspace = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        }
        if !cfg.workspace.is_absolute() {
            let parent = path.parent().unwrap_or_else(|| Path::new("."));
            cfg.workspace = parent.join(&cfg.workspace);
        }
        cfg.workspace = std::path::absolute(&cfg.workspace)
            .with_context(|| format!("invalid workspace {}", cfg.workspace.display()))?;
        empty_to_none(&mut cfg.auth_token);
        empty_to_none(&mut cfg.admin_password);
        empty_to_none(&mut cfg.client_id);
        empty_to_none(&mut cfg.client_secret);
        empty_to_none(&mut cfg.tunnel_token);
        empty_to_none(&mut cfg.tunnel_hostname);
        empty_to_none(&mut cfg.proxy.username);
        empty_to_none(&mut cfg.proxy.password);
        if cfg.executor.kind.trim().is_empty() {
            cfg.executor.kind = "opencode".to_string();
        }
        cfg.executor.kind = cfg.executor.kind.trim().to_ascii_lowercase();
        if cfg.executor.command.trim().is_empty() {
            cfg.executor.command = cfg.executor.kind.clone();
        }
        Ok(cfg)
    }

    pub fn save_to_path(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let body = toml::to_string_pretty(self).context("failed to serialize config")?;
        let text = format!(
            "# AgentBridge service config (~/.agentbridge/config.toml).\n\
             # Startup-level settings; changes take effect after `agentbridge restart`.\n\
             # Authentication: leave empty to ignore, fill in a value to use it.\n\
             {body}"
        );
        fs::write(path, text).with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }

    pub fn default_for_current_dir() -> Self {
        let workspace = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut cfg = Self::new(workspace);
        cfg.auth_token = None;
        cfg.admin_password = None;
        cfg.client_id = None;
        cfg.client_secret = None;
        cfg.tunnel_token = None;
        cfg.tunnel_hostname = None;
        cfg
    }

    pub fn load_or_create(path: &Path) -> Result<Self> {
        if !path.is_file() {
            let cfg = Self::default_for_current_dir();
            cfg.save_to_path(path)?;
            return Ok(cfg);
        }
        Self::load_from_path(path)
    }
}

pub fn load_or_create_user_config() -> Result<(Config, PathBuf)> {
    let path = user_config_path()?;
    let cfg = Config::load_or_create(&path)?;
    Ok((cfg, path))
}

fn empty_to_none(value: &mut Option<String>) {
    if value.as_ref().is_some_and(|s| s.trim().is_empty()) {
        *value = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_detection() {
        let mut cfg = Config::new(PathBuf::from("/tmp/ws"));
        assert!(cfg.is_loopback());
        cfg.host = "0.0.0.0".into();
        assert!(!cfg.is_loopback());
        cfg.host = "localhost".into();
        assert!(cfg.is_loopback());
    }

    #[test]
    fn executor_defaults_to_opencode() {
        let cfg = Config::new(PathBuf::from("/tmp/ws"));
        assert_eq!(cfg.executor.kind, "opencode");
        assert_eq!(cfg.executor.command, "opencode");
    }

    #[test]
    fn config_defaults_to_port_8040() {
        let cfg = Config::new(PathBuf::from("/tmp/ws"));
        assert_eq!(cfg.port, 8040);
    }
}