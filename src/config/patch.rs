// src/config/patch.rs
use anyhow::{bail, Result};
use crate::config::{Config, ExecutorMode};

#[derive(Debug, Clone, Default)]
pub struct ConfigPatch {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub no_auth: Option<bool>,
    pub auth_token: Option<String>,
    pub admin_password: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub executor_command: Option<String>,
    pub executor_mode: Option<ExecutorMode>,
}

pub fn patch_config(cfg: &mut Config, patch: ConfigPatch) -> Result<()> {
    if let Some(host) = patch.host {
        if host.trim().is_empty() {
            bail!("host is required");
        }
        cfg.host = host.trim().to_string();
    }
    if let Some(port) = patch.port {
        if port == 0 {
            bail!("port is required");
        }
        cfg.port = port;
    }
    if let Some(no_auth) = patch.no_auth {
        cfg.no_auth = no_auth;
    }
    cfg.auth_token = keep_secret(patch.auth_token, cfg.auth_token.clone());
    cfg.admin_password = keep_secret(patch.admin_password, cfg.admin_password.clone());
    cfg.client_id = keep_secret(patch.client_id, cfg.client_id.clone());
    cfg.client_secret = keep_secret(patch.client_secret, cfg.client_secret.clone());
    if let Some(command) = patch.executor_command {
        let command = command.trim();
        if !command.is_empty() {
            cfg.executor.command = command.to_string();
        }
    }
    if let Some(mode) = patch.executor_mode {
        cfg.executor.mode = mode;
    }
    Ok(())
}

pub fn keep_secret(incoming: Option<String>, current: Option<String>) -> Option<String> {
    match incoming {
        Some(value) if !value.trim().is_empty() => Some(value),
        _ => current,
    }
}

pub fn first_nonempty(
    cli: Option<String>,
    env_key: &str,
    from_toml: Option<String>,
) -> Option<String> {
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