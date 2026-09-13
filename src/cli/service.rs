// src/cli/service.rs
use std::time::Duration;

use anyhow::Result;

use agentbridge::config;
use agentbridge::service::{
    RestartOutcome, ServiceManager, ServiceStatus, StartOutcome, StopOutcome,
};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);

fn manager() -> Result<ServiceManager> {
    ServiceManager::native()
}

/// `agentbridge start`: launch the background service unless one is running.
pub fn start(host: Option<String>, port: Option<u16>) -> Result<()> {
    let (cfg, _config_path) = config::load_or_create_user_config()?;
    let host = host.unwrap_or(cfg.host);
    let port = port.unwrap_or(cfg.port);
    let manager = manager()?;
    let args = serve_args(&host, port);
    match manager.start(&host, port, args, STARTUP_TIMEOUT)? {
        StartOutcome::Started(record) => println!(
            "service    started (pid={}) on http://{}/",
            record.pid,
            record.listen_addr()
        ),
        StartOutcome::AlreadyRunning(record) => println!(
            "service    already running (pid={}) on http://{}/",
            record.pid,
            record.listen_addr()
        ),
        StartOutcome::AlreadyRunningUnmanaged { host, port } => println!(
            "service    something is already listening on {host}:{port}; refusing to start a second instance"
        ),
    }
    Ok(())
}

/// `agentbridge stop`: stop the recorded service, with `--force` override.
pub fn stop(force: bool) -> Result<()> {
    let manager = manager()?;
    match manager.stop(force)? {
        StopOutcome::Stopped(pid) => println!("service    stopped (pid={pid})"),
        StopOutcome::NotRunning => println!("service    not running"),
    }
    Ok(())
}

/// `agentbridge restart`: `stop` followed by `start`.
pub fn restart(host: Option<String>, port: Option<u16>, force: bool) -> Result<()> {
    let (cfg, _config_path) = config::load_or_create_user_config()?;
    let host = host.unwrap_or(cfg.host);
    let port = port.unwrap_or(cfg.port);
    let manager = manager()?;
    let args = serve_args(&host, port);
    let RestartOutcome { stopped_pid, start } =
        manager.restart(&host, port, args, STARTUP_TIMEOUT, force)?;
    if let Some(pid) = stopped_pid {
        println!("service    stopped (pid={pid})");
    }
    match start {
        StartOutcome::Started(record) => println!(
            "service    started (pid={}) on http://{}/",
            record.pid,
            record.listen_addr()
        ),
        StartOutcome::AlreadyRunning(record) => println!(
            "service    already running (pid={}) on http://{}/",
            record.pid,
            record.listen_addr()
        ),
        StartOutcome::AlreadyRunningUnmanaged { host, port } => println!(
            "service    something is already listening on {host}:{port}; refusing to start a second instance"
        ),
    }
    Ok(())
}

/// `agentbridge status`: report process lifecycle state only (no SQLite).
pub fn status() -> Result<()> {
    let manager = manager()?;
    match manager.status()? {
        ServiceStatus::Running(record) => {
            println!("service    running");
            println!("pid        {}", record.pid);
            println!("listen     {}", record.listen_addr());
            println!("version    {}", record.version);
            println!("started_at {}", record.started_at.to_rfc3339());
            println!("log        {}", manager.log_path().display());
        }
        ServiceStatus::Stale(record) => {
            println!("service    stopped (stale pid {})", record.pid);
            println!("note       run `agentbridge start` to clean up and restart");
        }
        ServiceStatus::Stopped => println!("service    stopped"),
    }
    Ok(())
}

fn serve_args(host: &str, port: u16) -> Vec<String> {
    vec![
        "serve".to_string(),
        "--host".to_string(),
        host.to_string(),
        "--port".to_string(),
        port.to_string(),
    ]
}
