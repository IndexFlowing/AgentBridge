// src/task/outcome.rs
//! Execution outcome recording, git changes collection, and test summary.

use std::path::Path;
use chrono::Utc;

use crate::executor::{ExecutorError, ExecutorOutcome};
use crate::git;
use crate::protocol::{C2cPlan, C2cState};
use crate::state::{BridgeState, TaskStatus, TestResult};
use crate::storage::Storage;

pub fn record_outcome(
    storage: &Storage,
    project_name: &str,
    workspace: &Path,
    _task_id: &str,
    plan: &C2cPlan,
    outcome: ExecutorOutcome,
) -> Result<(), ExecutorError> {
    let mut state = storage
        .load_task_state(project_name)
        .map_err(|e| ExecutorError::Other(e.to_string()))?;
    let now = Utc::now();
    let changed = collect_changed_files(workspace);
    let tests = finalize_tests(plan, &outcome);

    if outcome.cancelled {
        mark_cancelled(&mut state);
        state.changed_files = changed;
        state.tests = tests;
    } else if outcome.exit_code.unwrap_or(1) == 0 && outcome.error.is_none() {
        state.state = C2cState::Executed;
        state.task_status = Some(TaskStatus::Executed);
        state.status = Some("success".into());
        state.exit_code = outcome.exit_code;
        state.summary = Some(outcome.summary);
        state.error = None;
        state.finished_at = Some(now);
        state.changed_files = changed;
        state.tests = tests;
        state.updated_at = now;
    } else {
        state.state = C2cState::Executed;
        state.task_status = Some(TaskStatus::Failed);
        state.status = Some("failed".into());
        state.exit_code = outcome.exit_code.or(Some(1));
        state.summary = Some(outcome.summary);
        state.error = outcome.error;
        state.finished_at = Some(now);
        state.changed_files = changed;
        state.tests = tests;
        state.updated_at = now;
    }
    storage
        .save_task_state(project_name, &state)
        .map_err(|e| ExecutorError::Other(e.to_string()))?;
    state
        .write_c2c(
            workspace,
            notes_for(state.task_status.unwrap_or(TaskStatus::Failed)),
        )
        .map_err(|e| ExecutorError::Other(e.to_string()))?;

    // 打印精简的流程结束日志：修改了哪些文件、测试结果
    println!(
        "\n  ● [Task Finished] Status: {} (Exit Code: {})",
        state.status.as_deref().unwrap_or("completed"),
        outcome.exit_code.unwrap_or(-1)
    );
    if !state.changed_files.is_empty() {
        println!("    Changed Files ({}):", state.changed_files.len());
        for file in &state.changed_files {
            println!("      • {file}");
        }
    }
    if let Some(ref t) = state.tests {
        println!("    Tests: {} ({})", t.command, t.status);
    }
    println!();

    Ok(())
}

pub fn mark_cancelled(state: &mut BridgeState) {
    let now = Utc::now();
    state.state = C2cState::Cancelled;
    state.task_status = Some(TaskStatus::Cancelled);
    state.status = Some("cancelled".into());
    state.finished_at = Some(now);
    state.summary = Some("Executor was cancelled.".into());
    state.error = None;
    state.updated_at = now;
}

pub fn finalize_tests(plan: &C2cPlan, outcome: &ExecutorOutcome) -> Option<TestResult> {
    let command = plan.tests_command()?;
    let passed = outcome.exit_code == Some(0)
        && !outcome.cancelled
        && outcome
            .tests_excerpt
            .as_deref()
            .map(|s| !s.to_ascii_lowercase().contains("failed"))
            .unwrap_or(true);
    Some(TestResult {
        status: if outcome.cancelled {
            "cancelled".into()
        } else if passed {
            "passed".into()
        } else {
            "failed".into()
        },
        command,
        exit_code: outcome.exit_code,
        summary: outcome
            .tests_excerpt
            .clone()
            .or_else(|| Some(outcome.summary.clone())),
        timestamp: Utc::now(),
    })
}

pub fn collect_changed_files(workspace: &Path) -> Vec<String> {
    if !git::is_repository(workspace) {
        return Vec::new();
    }
    match git::status(workspace) {
        Ok(st) => {
            let mut files = st.changed_files;
            files.extend(st.untracked_files);
            files.sort();
            files.dedup();
            files
        }
        Err(_) => Vec::new(),
    }
}

pub fn notes_for(status: TaskStatus) -> Option<&'static str> {
    match status {
        TaskStatus::Planned | TaskStatus::Created => Some("Executor: implement this PLAN in the workspace, run TESTS, then stop. Do not paste source."),
        TaskStatus::Running => Some("Executor is running. Poll task_status; inspect the workspace through MCP when it finishes."),
        TaskStatus::Executed | TaskStatus::Failed => Some("Please inspect git_diff, test_status, and execution_summary through MCP."),
        TaskStatus::Cancelled => Some("Executor was cancelled."),
        _ => None,
    }
}