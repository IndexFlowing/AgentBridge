// src/adapters/cli/init.rs
use std::fs;
use std::path::PathBuf;
use anyhow::{bail, Context, Result};

use crate::config::{self, Config};

pub fn run(workspace: Option<PathBuf>, port: u16, local_only: bool) -> Result<()> {
    let workspace = match workspace {
        Some(p) => p,
        None => std::env::current_dir().context("failed to determine current directory")?,
    };
    let workspace = std::path::absolute(&workspace)?;
    if workspace.exists() && !workspace.is_dir() {
        bail!(
            "workspace path exists but is not a directory: {}",
            workspace.display()
        );
    }
    if !workspace.exists() {
        fs::create_dir_all(&workspace).with_context(|| {
            format!(
                "failed to create workspace directory {}",
                workspace.display()
            )
        })?;
    }

    let mut cfg = Config::new(workspace.clone());
    cfg.port = port;

    let project_cfg = workspace.join(".agentbridge.toml");
    cfg.save_to_path(&project_cfg)?;
    println!("wrote {}", project_cfg.display());

    if !local_only {
        let user_cfg = config::user_config_path()?;
        cfg.save_to_path(&user_cfg)?;
        println!("wrote {}", user_cfg.display());
    }

    let _ = fs::create_dir_all(config::state_dir(&workspace));
    println!();
    println!("Workspace: {}", workspace.display());
    println!("Edit `.agentbridge.toml` to set admin_password (empty = unused).");
    println!("Start the Brain endpoint with:\n  agentbridge serve\n");
    println!("Default MCP URL: {}", cfg.mcp_url());
    Ok(())
}
