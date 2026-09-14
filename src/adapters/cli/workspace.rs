use anyhow::{Context, Result};
use std::sync::Arc;

use crate::config;
use crate::core::AppCore;
use crate::git;

pub fn run() -> Result<()> {
    let (cfg, _config_path) = config::load_or_create_user_config()?;
    let cfg = Arc::new(cfg);
    let core = AppCore::bootstrap(cfg.clone())?;

    let projects = core.projects.list()?;
    let default_project = projects
        .first()
        .context("No projects mounted. Please use Web UI to mount a workspace.")?;
    let project_name = default_project.name.clone();
    let project = core
        .hub
        .get(&project_name)
        .context("default project not found")?;

    let info = project.workspace.info();
    let state = core.tasks.task_state(&project_name).unwrap_or_default();

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
        match git::status(project.workspace.root()) {
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
