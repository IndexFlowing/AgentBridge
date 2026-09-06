use std::path::PathBuf;
use anyhow::Result;

use agentbridge::config;
use agentbridge::git;
use agentbridge::state::BridgeState;
use agentbridge::workspace::Workspace;

pub fn run(config_path: Option<PathBuf>) -> Result<()> {
    let (cfg, path) = config::find_config(config_path.as_deref())?;
    let ws = Workspace::open(
        &cfg.workspace,
        cfg.security.max_file_size,
        cfg.security.deny_sensitive_files,
    )?;
    let info = ws.info();
    let state = BridgeState::load(ws.root())?;

    println!("config     {}", path.display());
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
    println!("git repo   {}", info.git_repository);
    if info.git_repository {
        match git::status(ws.root()) {
            Ok(st) => {
                println!(
                    "git        {} ({})",
                    st.branch,
                    if st.clean { "clean" } else { "dirty" }
                );
                if !st.changed_files.is_empty() {
                    println!("changed    {}", st.changed_files.join(", "));
                }
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
    if let Some(executor) = &state.executor {
        println!("executor   {executor}");
    }
    if let Some(status) = &state.status {
        println!("status     {status}");
    }
    if let Some(code) = state.exit_code {
        println!("exit_code  {code}");
    }
    if let Some(summary) = &state.summary {
        println!("summary    {summary}");
    }
    if let Some(tests) = &state.tests {
        println!(
            "tests      {} ({}) exit={:?}",
            tests.command, tests.status, tests.exit_code
        );
    }
    Ok(())
}