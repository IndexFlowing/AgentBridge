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
    #[serde(default)]
    pub allow_any_host: bool,
    /// Optional static Bearer token accepted on `/mcp`. Empty = unused.
    #[serde(default)]
    pub auth_token: Option<String>,
    /// Password for `/oauth/authorize`. Empty = generate a PIN at startup.
    #[serde(default)]
    pub admin_password: Option<String>,
    /// Optional pre-registered OAuth client_id. Empty = dynamic registration.
    #[serde(default)]
    pub client_id: Option<String>,
    /// Optional pre-registered OAuth client_secret. Empty = unused.
    #[serde(default)]
    pub client_secret: Option<String>,
    /// Disable the `/mcp` 401 challenge (localhost / `--dev` only).
    #[serde(default)]
    pub no_auth: bool,
    /// Cloudflare named-tunnel token (`cloudflared tunnel run --token`). Empty = quick tunnel.
    #[serde(default)]
    pub tunnel_token: Option<String>,
    /// Stable public hostname for a named tunnel (e.g. `mcp.example.com`). Empty = unused.
    #[serde(default)]
    pub tunnel_hostname: Option<String>,
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
    /// Execution output mode: `stream` (live print to terminal, default) or `silent` (quiet background)
    #[serde(default = "default_executor_mode")]
    pub mode: ExecutorMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecutorMode {
    Stream,
    Silent,
}

impl Default for ExecutorMode {
    fn default() -> Self {
        Self::Stream
    }
}

fn default_executor_mode() -> ExecutorMode {
    ExecutorMode::Stream
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            kind: default_executor_type(),
            command: default_executor_command(),
            mode: default_executor_mode(),
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
            allow_any_host: false,
            auth_token: Some(String::new()),
            admin_password: Some(String::new()),
            client_id: Some(String::new()),
            client_secret: Some(String::new()),
            no_auth: false,
            tunnel_token: Some(String::new()),
            tunnel_hostname: Some(String::new()),
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
        empty_to_none(&mut cfg.auth_token);
        empty_to_none(&mut cfg.admin_password);
        empty_to_none(&mut cfg.client_id);
        empty_to_none(&mut cfg.client_secret);
        empty_to_none(&mut cfg.tunnel_token);
        empty_to_none(&mut cfg.tunnel_hostname);
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
        let body = toml::to_string_pretty(self).context("failed to serialize config")?;
        let text = format!(
            "# AgentBridge project config.\n\
             # Authentication: leave empty to ignore, fill in a value to use it.\n\
             {body}"
        );
        fs::write(path, text).with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }
}

fn empty_to_none(value: &mut Option<String>) {
    if value.as_ref().is_some_and(|s| s.trim().is_empty()) {
        *value = None;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiPrefs {
    #[serde(default = "default_true")]
    pub auto_start: bool,
    #[serde(default)]
    pub start_tunnel: bool,
}

impl Default for UiPrefs {
    fn default() -> Self {
        Self {
            auto_start: true,
            start_tunnel: false,
        }
    }
}

pub fn ui_prefs_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("cannot determine home directory")?;
    Ok(home.join(".agentbridge").join("ui.toml"))
}

pub fn load_ui_prefs() -> UiPrefs {
    let Ok(path) = ui_prefs_path() else {
        return UiPrefs::default();
    };
    let Ok(text) = fs::read_to_string(path) else {
        return UiPrefs::default();
    };
    toml::from_str(&text).unwrap_or_default()
}

pub fn save_ui_prefs(prefs: &UiPrefs) -> Result<()> {
    let path = ui_prefs_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, toml::to_string_pretty(prefs)?).context("failed to write ui.toml")?;
    Ok(())
}

/// CLI flag, then process environment, then toml. Empty strings are ignored.
pub fn first_nonempty(cli: Option<String>, env_key: &str, from_toml: Option<String>) -> Option<String> {
    cli.map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            std::env::var(env_key)
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .or_else(|| {
            from_toml
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
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

    #[test]
    fn init_toml_lists_empty_auth_fields() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join(".agentbridge.toml");
        Config::new(dir.path().to_path_buf())
            .save_to_path(&path)
            .unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("admin_password"));
        assert!(text.contains("auth_token"));
        assert!(text.contains("client_id"));
        assert!(text.contains("client_secret"));
        assert!(text.contains("no_auth"));
        let loaded = Config::load_from_path(&path).unwrap();
        assert!(loaded.admin_password.is_none());
        assert!(loaded.auth_token.is_none());
        assert!(loaded.client_id.is_none());
        assert!(loaded.client_secret.is_none());
        assert!(!loaded.no_auth);
    }

    #[test]
    fn filled_auth_fields_are_loaded() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join(".agentbridge.toml");
        let mut cfg = Config::new(dir.path().to_path_buf());
        cfg.admin_password = Some("my-pin".into());
        cfg.auth_token = Some("static-token".into());
        cfg.save_to_path(&path).unwrap();
        let loaded = Config::load_from_path(&path).unwrap();
        assert_eq!(loaded.admin_password.as_deref(), Some("my-pin"));
        assert_eq!(loaded.auth_token.as_deref(), Some("static-token"));
    }
}
