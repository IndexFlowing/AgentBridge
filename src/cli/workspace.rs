// src/cli/workspace.rs
use anyhow::{Context, Result};

use agentbridge::config;
use agentbridge::git;
use agentbridge::storage::Storage;
use agentbridge::workspace::Workspace;

pub fn run() -> Result<()> {
    let (cfg, _config_path) = config::load_or_create_user_config()?;
    let storage = Storage::init()?;
    let projects = storage.load_projects()?;

    let default_project = projects
        .first()
        .context("No projects mounted. Please use Web UI to mount a workspace.")?;

    let ws = Workspace::open(&default_project.path, 1_048_576, true)?;

    let info = ws.info();
    let state = storage
        .load_task_state(&default_project.name)
        .unwrap_or_default();

    println!("workspace  {}", info.workspace);
    println!(
        "project    {}",
        if info.project_type.is_empty() {
            "unknown".into()
        } else {
            info.project_type.join(", ")
        }
    );
    println!("mcp        {}", cfg.mcp_url());

    if info.git_repository {
        match git::status(ws.root()) {
            Ok(st) => {
                println!(
                    "git        {} ({})",
                    st.branch,
                    if st.clean { "clean" } else { "dirty" }
                );
            }
            Err(err) => println!("git        {err}"),
        }
    }

    println!(
        "task       {}  iteration={}  state={}  lifecycle={}",
        state.task_id.as_deref().unwrap_or("(none)"),
        state.iteration,
        state.state,
        state.task_status.map(|s| s.as_str()).unwrap_or("none")
    );

    Ok(())
}
