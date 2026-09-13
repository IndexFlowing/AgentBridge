// src/config/types.rs
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::config::paths::ui_prefs_path;

pub const DEFAULT_HOST: &str = "127.0.0.1";

#[cfg(debug_assertions)]
pub const DEFAULT_PORT: u16 = 8040;

// 发布/安装模式（cargo install / cargo build --release）默认 8030
#[cfg(not(debug_assertions))]
pub const DEFAULT_PORT: u16 = 8030;

pub const DEFAULT_LOG_LEVEL: &str = "info";
pub const DEFAULT_MAX_FILE_SIZE: u64 = 1_048_576;
pub const DEFAULT_MAX_DIFF_BYTES: usize = 65_536;
pub const DEFAULT_MAX_SEARCH_RESULTS: usize = 50;
pub const ALLOWED_EXECUTOR_TYPES: &[&str] = &["opencode", "codex", "claude"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutorDefinition {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub command: String,
    #[serde(default)]
    pub executable: Option<PathBuf>,
    #[serde(default)]
    pub working_directory: Option<PathBuf>,
    #[serde(default)]
    pub proxy_id: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl ExecutorDefinition {
    pub fn new(name: String, kind: String, command: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            display_name: name.clone(),
            name,
            kind: kind.trim().to_ascii_lowercase(),
            command: command.trim().to_string(),
            executable: None,
            working_directory: None,
            proxy_id: None,
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorConfig {
    #[serde(rename = "type", default = "default_executor_type")]
    pub kind: String,
    #[serde(default = "default_executor_command")]
    pub command: String,
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

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            kind: default_executor_type(),
            command: default_executor_command(),
            mode: default_executor_mode(),
        }
    }
}

fn default_executor_mode() -> ExecutorMode {
    ExecutorMode::Stream
}

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
        }
    }
}

pub(crate) fn default_host() -> String {
    DEFAULT_HOST.to_string()
}

pub(crate) fn default_port() -> u16 {
    DEFAULT_PORT
}

fn default_log_level() -> String {
    DEFAULT_LOG_LEVEL.to_string()
}

fn default_max_file_size() -> u64 {
    DEFAULT_MAX_FILE_SIZE
}

pub(crate) fn default_true() -> bool {
    true
}

fn default_max_diff_bytes() -> usize {
    DEFAULT_MAX_DIFF_BYTES
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
