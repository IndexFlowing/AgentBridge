// src/task/outcome.rs
//! Execution outcome recording, git changes collection, and test summary.
//!
//! This module is the single owner of the rules that turn a finished run into
//! persisted task state. Both the live executor supervision path
//! ([`record_outcome`]) and an externally reported result
//! (`TaskService::record_executed`) funnel through [`apply`] so success/failure
//! derivation, changed-file collection, and test-result construction are
//! maintained in exactly one place.

use chrono::Utc;
use std::path::Path;

use crate::executor::{ExecutorError, ExecutorOutcome};
use crate::git;
use crate::infra::notification::send_task_notification;
use crate::protocol::{C2cPlan, C2cState};
use crate::state::{BridgeState, TaskStatus, TestResult};
use crate::storage::Storage;

/// Normalized description of a finished task run.
///
/// Produced by each entry path (executor supervision, externally reported
/// result) and consumed only by [`apply`]. It deliberately carries no policy:
/// deciding what a run means and persisting it is [`apply`]'s job.
#[derive(Debug, Clone)]
pub struct OutcomeReport {
    pub task_status: TaskStatus,
    pub c2c_state: C2cState,
    pub exit_code: Option<i32>,
    /// Task-level summary. `None` keeps the previously persisted summary.
    pub summary: Option<String>,
    /// Task-level error. `None` keeps the previously persisted error;
    /// `Some(value)` sets it (and `Some(None)` clears it).
    pub error: Option<Option<String>>,
    /// Explicit changed files. `None` collects them from git.
    pub changed_files: Option<Vec<String>>,
    pub tests_command: Option<String>,
    pub tests_summary: Option<String>,
    /// Raw test output used only to decide pass/fail; never persisted verbatim.
    pub tests_excerpt: Option<String>,
}

impl OutcomeReport {
    /// Derive the canonical report from a live executor outcome.
    ///
    /// Owns the success/failure/cancelled rules shared by the task domain.
    pub fn from_executor(plan: &C2cPlan, outcome: ExecutorOutcome) -> Self {
        let tests_summary = outcome
            .tests_excerpt
            .clone()
            .or_else(|| Some(outcome.summary.clone()));

        if outcome.cancelled {
            return Self {
                task_status: TaskStatus::Cancelled,
                c2c_state: C2cState::Cancelled,
                exit_code: None,
                summary: Some("Executor was cancelled.".into()),
                error: Some(None),
                changed_files: None,
                tests_command: plan.tests_command(),
                tests_summary,
                tests_excerpt: outcome.tests_excerpt,
            };
        }

        let success = outcome.exit_code == Some(0) && outcome.error.is_none();
        let (task_status, exit_code, error) = if success {
            (TaskStatus::Executed, outcome.exit_code, Some(None))
        } else {
            (
                TaskStatus::Failed,
                outcome.exit_code.or(Some(1)),
                Some(outcome.error),
            )
        };

        Self {
            task_status,
            c2c_state: C2cState::Executed,
            exit_code,
            summary: Some(outcome.summary),
            error,
            changed_files: None,
            tests_command: plan.tests_command(),
            tests_summary,
            tests_excerpt: outcome.tests_excerpt,
        }
    }
}

/// Apply the single set of outcome rules to a task record.
///
/// Callers only normalize their input into an [`OutcomeReport`]; every state
/// transition, changed-file decision, and test-result verdict happens here.
pub fn apply(state: &mut BridgeState, workspace: &Path, mut report: OutcomeReport) {
    let now = Utc::now();
    state.state = report.c2c_state;
    state.task_status = Some(report.task_status);
    state.status = Some(report.task_status.result_status().to_string());
    state.exit_code = report.exit_code;
    if let Some(summary) = report.summary.take() {
        state.summary = Some(summary);
    }
    if let Some(error) = report.error.take() {
        state.error = error;
    }
    state.finished_at = Some(now);
    let test_result = build_test_result(&report, now);
    if let Some(files) = report.changed_files.take() {
        state.changed_files = files;
    } else if git::is_repository(workspace) {
        state.changed_files = collect_changed_files(workspace);
    }
    if let Some(result) = test_result {
        state.tests = Some(result);
    }
    state.updated_at = now;
}

fn build_test_result(report: &OutcomeReport, now: chrono::DateTime<Utc>) -> Option<TestResult> {
    let command = report.tests_command.clone()?;
    let passed = report.task_status == TaskStatus::Executed
        && report.exit_code == Some(0)
        && !tests_excerpt_reports_failure(report.tests_excerpt.as_deref());
    let status = if report.task_status == TaskStatus::Cancelled {
        "cancelled"
    } else if passed {
        "passed"
    } else {
        "failed"
    };
    Some(TestResult {
        status: status.into(),
        command,
        exit_code: report.exit_code,
        summary: report.tests_summary.clone(),
        timestamp: now,
    })
}

pub fn record_outcome(
    storage: &Storage,
    project_name: &str,
    workspace: &Path,
    task_id: &str,
    plan: &C2cPlan,
    outcome: ExecutorOutcome,
) -> Result<(), ExecutorError> {
    let mut state = storage
        .load_task_state_by_id(task_id)
        .map_err(|e| ExecutorError::Other(e.to_string()))?
        .unwrap_or(
            storage
                .load_task_state(project_name)
                .map_err(|e| ExecutorError::Other(e.to_string()))?,
        );

    let exit_code = outcome.exit_code;
    let report = OutcomeReport::from_executor(plan, outcome);
    apply(&mut state, workspace, report);

    storage
        .save_task_state(project_name, &state)
        .map_err(|e| ExecutorError::Other(e.to_string()))?;

    // 打印精简的流程结束日志：修改了哪些文件、测试结果
    println!(
        "\n  ● [{}][Task Finished] Status: {} (Exit Code: {})",
        project_name,
        state.status.as_deref().unwrap_or("completed"),
        exit_code.unwrap_or(-1)
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

    // 2. 异步触发系统原生桌面弹窗通知（非阻塞）
    let test_summary = state
        .tests
        .as_ref()
        .map(|t| format!("{} ({})", t.command, t.status));

    send_task_notification(
        project_name,
        &plan.goal,
        state.task_status.unwrap_or(TaskStatus::Executed),
        state.changed_files.len(),
        test_summary.as_deref(),
    );

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

/// Decide whether a captured test excerpt reports a real failure.
///
/// Cargo-style summaries contain the word "failed" even on success
/// (`test result: ok. 119 passed; 0 failed`), so a bare substring match would
/// incorrectly mark passing runs as failed. Only explicit failure markers or a
/// non-zero failure count are treated as failures.
fn tests_excerpt_reports_failure(excerpt: Option<&str>) -> bool {
    let Some(excerpt) = excerpt else {
        return false;
    };
    let lower = excerpt.to_ascii_lowercase();
    if lower.contains("test result: failed")
        || lower.contains("error: test failed")
        || lower.contains("failures:")
    {
        return true;
    }
    let tokens: Vec<&str> = lower.split_whitespace().collect();
    tokens.windows(2).any(|pair| {
        let label = pair[1].trim_matches(|c: char| !c.is_ascii_alphanumeric());
        matches!(label, "failed" | "failures") && matches!(pair[0].parse::<u64>(), Ok(n) if n > 0)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> C2cPlan {
        C2cPlan::new(
            "c2c_test".into(),
            1,
            "goal".into(),
            vec!["implement".into()],
            vec!["cargo test".into()],
            "done".into(),
        )
        .unwrap()
    }

    fn outcome(exit_code: i32, excerpt: Option<&str>) -> ExecutorOutcome {
        ExecutorOutcome {
            exit_code: Some(exit_code),
            summary: "summary".into(),
            tests_excerpt: excerpt.map(str::to_string),
            cancelled: false,
            error: None,
        }
    }

    fn recorded_status(out: ExecutorOutcome) -> String {
        let report = OutcomeReport::from_executor(&plan(), out);
        build_test_result(&report, Utc::now()).unwrap().status
    }

    #[test]
    fn passing_excerpt_with_zero_failed_is_not_a_failure() {
        let out = outcome(
            0,
            Some("test result: ok. 119 passed; 0 failed; 0 ignored; 0 measured"),
        );
        assert_eq!(recorded_status(out), "passed");
    }

    #[test]
    fn non_zero_failed_count_is_a_failure() {
        let out = outcome(
            1,
            Some("test result: FAILED. 1 passed; 2 failed; 0 ignored"),
        );
        assert_eq!(recorded_status(out), "failed");
    }

    #[test]
    fn cancelled_run_reports_cancelled() {
        let mut out = outcome(1, None);
        out.cancelled = true;
        assert_eq!(recorded_status(out), "cancelled");
    }

    #[test]
    fn failed_run_records_failed_status_and_derived_exit_code() {
        let report = OutcomeReport::from_executor(&plan(), outcome(7, None));
        assert_eq!(report.task_status, TaskStatus::Failed);
        assert_eq!(report.exit_code, Some(7));
    }

    #[test]
    fn successful_run_records_executed_status() {
        let report = OutcomeReport::from_executor(&plan(), outcome(0, None));
        assert_eq!(report.task_status, TaskStatus::Executed);
        assert_eq!(report.exit_code, Some(0));
    }
}
