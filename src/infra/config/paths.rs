// src/config/paths.rs
use crate::config::Config;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

pub fn user_config_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("cannot determine home directory")?;
    Ok(home.join(".agentbridge").join("config.toml"))
}

pub fn project_config_path() -> PathBuf {
    PathBuf::from(".agentbridge.toml")
}

pub fn system_config_path() -> PathBuf {
    PathBuf::from("/etc/agentbridge/config.toml")
}

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

    if let Ok(user) = user_config_path() {
        if user.is_file() {
            let cfg = Config::load_from_path(&user)?;
            return Ok((cfg, user));
        }
    }

    #[cfg(unix)]
    {
        let sys = system_config_path();
        if sys.is_file() {
            let cfg = Config::load_from_path(&sys)?;
            return Ok((cfg, sys));
        }
    }

    bail!(
        "no AgentBridge config found. Run `agentbridge init <workspace>` first, or pass --config."
    )
}

pub fn sidecar_config_path(explicit: Option<&Path>) -> PathBuf {
    if let Some(path) = explicit {
        return path.to_path_buf();
    }
    find_config(None)
        .map(|(_, path)| path)
        .unwrap_or_else(|_| project_config_path())
}

/// The AgentBridge-managed Agent root (`~/.agentbridge/.agent`).
///
/// This is the single long-term authoritative home for `agent.yaml`, `rules/`,
/// `skills/`, and `projects/*.yaml`. Workspace projects never own `.agent`
/// configuration.
pub fn agent_root() -> Result<PathBuf> {
    let home = dirs::home_dir().context("cannot determine home directory")?;
    Ok(home.join(".agentbridge").join(".agent"))
}

/// Create the Agent root (and its `projects/` profile directory) if missing.
pub fn ensure_agent_root() -> Result<PathBuf> {
    let root = agent_root()?;
    std::fs::create_dir_all(root.join("projects"))
        .with_context(|| format!("failed to create agent root {}", root.display()))?;
    Ok(root)
}

/// Directory holding rendered per-task C2C handoff snapshots.
///
/// These are AgentBridge-managed, non-authoritative rendering artifacts; the
/// authoritative task state lives in SQLite. They are never written into a
/// workspace.
pub fn handoff_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("cannot determine home directory")?;
    Ok(home.join(".agentbridge").join("handoff"))
}

/// Absolute path of the handoff snapshot for `task_id`.
pub fn handoff_path(task_id: &str) -> Result<PathBuf> {
    let safe: String = task_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    Ok(handoff_dir()?.join(format!("{safe}.c2c")))
}

pub fn state_dir(workspace: &Path) -> PathBuf {
    workspace.join(".agentbridge")
}

pub fn state_path(workspace: &Path) -> PathBuf {
    state_dir(workspace).join("state.json")
}

pub fn executor_pid_path(workspace: &Path) -> PathBuf {
    state_dir(workspace).join("executor.pid")
}

pub fn ui_prefs_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("cannot determine home directory")?;
    Ok(home.join(".agentbridge").join("ui.toml"))
}
