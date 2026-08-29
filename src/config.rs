use std::fs;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

pub const DEFAULT_HOST: &str = "127.0.0.1";
pub const DEFAULT_PORT: u16 = 8030;
pub const DEFAULT_MAX_FILE_SIZE: u64 = 1_048_576;
pub const DEFAULT_MAX_DIFF_BYTES: usize = 65_536;
pub const DEFAULT_MAX_SEARCH_RESULTS: usize = 50;

/// On-disk configuration for AgentBridge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub workspace: PathBuf,
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    /// Optional bearer token required on `/mcp`. Leave empty for local-only use.
    #[serde(default)]
    pub auth_token: Option<String>,
    #[serde(default)]
    pub executor: ExecutorConfig,
    #[serde(default)]
    pub security: SecurityConfig,
}

/// Local coding-agent adapter. `type` is allowlisted; `command` comes from this
/// file, never from an MCP request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorConfig {
    /// Allowlisted kinds: `opencode`, `codex`, `claude`. V0.2 implements OpenCode only.
    #[serde(rename = "type", default = "default_executor_type")]
    pub kind: String,
    /// Executable name or absolute path. Ignored if supplied via MCP.
    #[serde(default = "default_executor_command")]
    pub command: String,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            kind: default_executor_type(),
            command: default_executor_command(),
        }
    }
}

pub const ALLOWED_EXECUTOR_TYPES: &[&str] = &["opencode", "codex", "claude"];

fn default_executor_type() -> String {
    "opencode".to_string()
}

fn default_executor_command() -> String {
    "opencode".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    #[serde(default = "default_max_file_size")]
    pub max_file_size: u64,
    #[serde(default = "default_true")]
    pub deny_sensitive_files: bool,
    #[serde(default = "default_max_diff_bytes")]
    pub max_diff_bytes: usize,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            max_file_size: DEFAULT_MAX_FILE_SIZE,
            deny_sensitive_files: true,
            max_diff_bytes: DEFAULT_MAX_DIFF_BYTES,
        }
    }
}

fn default_host() -> String {
    DEFAULT_HOST.to_string()
}

fn default_port() -> u16 {
    DEFAULT_PORT
}

fn default_max_file_size() -> u64 {
    DEFAULT_MAX_FILE_SIZE
}

fn default_true() -> bool {
    true
}

fn default_max_diff_bytes() -> usize {
    DEFAULT_MAX_DIFF_BYTES
}

impl Config {
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            host: default_host(),
            port: default_port(),
            auth_token: None,
            executor: ExecutorConfig::default(),
            security: SecurityConfig::default(),
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
            bail!("config is missing workspace");
        }
        if !cfg.workspace.is_absolute() {
            let parent = path.parent().unwrap_or_else(|| Path::new("."));
            cfg.workspace = parent.join(&cfg.workspace);
        }
        cfg.workspace = std::path::absolute(&cfg.workspace)
            .with_context(|| format!("invalid workspace {}", cfg.workspace.display()))?;
        if let Some(token) = cfg.auth_token.as_mut() {
            if token.is_empty() {
                cfg.auth_token = None;
            }
        }
        if cfg.executor.kind.trim().is_empty() {
            cfg.executor.kind = default_executor_type();
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
        let text = toml::to_string_pretty(self).context("failed to serialize config")?;
        fs::write(path, text).with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }
}

/// User-level config: `~/.agentbridge/config.toml`.
pub fn user_config_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("cannot determine home directory")?;
    Ok(home.join(".agentbridge").join("config.toml"))
}

/// Project-level config: `<cwd>/.agentbridge.toml`.
pub fn project_config_path() -> PathBuf {
    PathBuf::from(".agentbridge.toml")
}

/// Resolve config: `--config`, then `./.agentbridge.toml`, then `~/.agentbridge/config.toml`.
pub fn find_config(explicit: Option<&Path>) -> Result<(Config, PathBuf)> {
    if let Some(path) = explicit {
        let cfg = Config::load_from_path(path)?;
        return Ok((cfg, path.to_path_buf()));
    }

    let project = project_config_path();
    if project.is_file() {
        let cfg = Config::load_from_path(&project)?;
        return Ok((cfg, std::path::absolute(project)?));
    }

    let user = user_config_path()?;
    if user.is_file() {
        let cfg = Config::load_from_path(&user)?;
        return Ok((cfg, user));
    }

    bail!(
        "no AgentBridge config found. Run `agentbridge init <workspace>` first, or pass --config."
    )
}

pub fn state_dir(workspace: &Path) -> PathBuf {
    workspace.join(".agentbridge")
}

pub fn state_path(workspace: &Path) -> PathBuf {
    state_dir(workspace).join("state.json")
}

pub fn current_c2c_path(workspace: &Path) -> PathBuf {
    state_dir(workspace).join("current.c2c")
}

pub fn executor_pid_path(workspace: &Path) -> PathBuf {
    state_dir(workspace).join("executor.pid")
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
    fn executor_section_roundtrip() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        let mut cfg = Config::new(dir.path().to_path_buf());
        cfg.executor.kind = "opencode".into();
        cfg.executor.command = "opencode".into();
        cfg.save_to_path(&path).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("[executor]"));
        assert!(text.contains("type"));
        let loaded = Config::load_from_path(&path).unwrap();
        assert_eq!(loaded.executor.kind, "opencode");
        assert_eq!(loaded.executor.command, "opencode");
    }
}
