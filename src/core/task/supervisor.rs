// src/task/supervisor.rs
//! Low-level process supervision, PID tracking, and graceful termination.

use std::fs;
use std::path::Path;
use std::time::Duration;

use crate::config;
use crate::executor::{process_is_alive, ExecutorError};
use crate::task::ActiveTask;

pub fn fail_if_running(guard: &Option<ActiveTask>, workspace: &Path) -> Result<(), ExecutorError> {
    if let Some(active) = guard {
        if process_is_alive(active.pid) {
            return Err(ExecutorError::AlreadyRunning(active.task_id.clone()));
        }
    }
    if let Some(pid) = read_pid(workspace) {
        if process_is_alive(pid) {
            return Err(ExecutorError::AlreadyRunning("unknown".into()));
        }
        clear_pid(workspace);
    }
    Ok(())
}

pub fn write_pid(workspace: &Path, pid: u32) -> Result<(), ExecutorError> {
    let dir = config::state_dir(workspace);
    fs::create_dir_all(&dir).map_err(|e| ExecutorError::Other(e.to_string()))?;
    fs::write(config::executor_pid_path(workspace), pid.to_string())
        .map_err(|e| ExecutorError::Other(e.to_string()))
}

pub fn read_pid(workspace: &Path) -> Option<u32> {
    fs::read_to_string(config::executor_pid_path(workspace))
        .ok()?
        .trim()
        .parse()
        .ok()
}

pub fn clear_pid(workspace: &Path) {
    let _ = fs::remove_file(config::executor_pid_path(workspace));
}

pub async fn wait_until_dead(pid: u32) {
    for _ in 0..20 {
        if !process_is_alive(pid) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}