// src/task.rs
use std::fs;
use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::config::{self, ExecutorMode};
use crate::executor::{
    kill_process_tree, process_is_alive, run_spawned, ExecutorError, ExecutorOutcome,
    SharedExecutorRegistry,
};
use crate::git;
use crate::protocol::{C2cPlan, C2cState};
use crate::state::{new_task_id, BridgeState, TaskStatus, TestResult};
use crate::storage::Storage;
use crate::workspace::Workspace;

#[derive(Clone)]
pub struct TaskRuntime {
    pub project_name: String,
    workspace: Arc<Workspace>,
    default_executor: String,
    registry: SharedExecutorRegistry,
    current: Arc<Mutex<Option<ActiveTask>>>,
    mode: ExecutorMode,
    storage: Arc<Storage>,
}

struct ActiveTask {
    task_id: String,
    pid: u32,
    cancel: CancellationToken,
    join: Option<tokio::task::JoinHandle<()>>,
}

impl TaskRuntime {
    pub fn new(
        project_name: String,
        workspace: Arc<Workspace>,
        default_executor: String,
        registry: SharedExecutorRegistry,
        mode: ExecutorMode,
        storage: Arc<Storage>,
    ) -> Result<Self, ExecutorError> {
        Ok(Self {
            project_name,
            workspace,
            default_executor,
            registry,
            current: Arc::new(Mutex::new(None)),
            mode,
            storage,
        })
    }

    pub async fn start_task(
        &self,
        goal: String,
        plan: PlanInput,
        executor_override: Option<&str>,
    ) -> Result<BridgeState, ExecutorError> {
        let executor_id = executor_override.unwrap_or(&self.default_executor);
        let registry = self.registry.read().unwrap().clone();
        let executor = registry
            .get(executor_id)
            .ok_or_else(|| ExecutorError::NotFound(executor_id.to_string()))?;
        let proxy = registry.resolve_proxy_for(executor_id);

        executor.detect()?;
        let mut guard = self.current.lock().await;
        self.fail_if_running(&guard)?;

        let mut state = self.load()?;
        let (task_id, iteration) = next_identity(&state);
        let plan = C2cPlan::new(
            task_id.clone(),
            iteration,
            goal,
            plan.actions,
            plan.tests,
            plan.success_criteria,
        )
        .map_err(|e| ExecutorError::Other(e.to_string()))?;

        state.apply_plan(
            &plan,
            &self.workspace.root().display().to_string(),
            executor.name(),
        );
        self.persist(&state)?;

        let spawned = match executor.start_task(&plan, self.workspace.root(), proxy.as_ref()) {
            Ok(s) => s,
            Err(err) => {
                self.mark_failed(&mut state, &err);
                let _ = self.persist(&state);
                return Err(err);
            }
        };

        state.mark_running();
        self.persist(&state)?;
        write_pid(self.workspace.root(), spawned.pid)?;

        let cancel = CancellationToken::new();
        let join = self.spawn_waiter(
            spawned.child,
            spawned.pid,
            task_id.clone(),
            plan.clone(),
            cancel.clone(),
        );
        *guard = Some(ActiveTask {
            task_id: task_id.clone(),
            pid: spawned.pid,
            cancel,
            join: Some(join),
        });

        Ok(state)
    }

    pub async fn status(&self, task_id: Option<&str>) -> Result<BridgeState, ExecutorError> {
        self.reap_if_finished().await;
        let state = self.load()?;
        if let Some(want) = task_id {
            if state.task_id.as_deref() != Some(want) {
                return Err(ExecutorError::Other(format!(
                    "unknown task_id {want} (current is {})",
                    state.task_id.as_deref().unwrap_or("(none)")
                )));
            }
        }
        Ok(state)
    }

    pub async fn cancel(&self, task_id: Option<&str>) -> Result<BridgeState, ExecutorError> {
        let mut guard = self.current.lock().await;
        let Some(active) = guard.as_mut() else {
            if let Some(pid) = read_pid(self.workspace.root()) {
                if process_is_alive(pid) {
                    let _ = kill_process_tree(pid);
                    let _ = wait_until_dead(pid).await;
                }
                clear_pid(self.workspace.root());
                let mut state = self.load()?;
                mark_cancelled(&mut state);
                self.persist(&state)?;
                return Ok(state);
            }
            return Err(ExecutorError::NotRunning);
        };
        if let Some(want) = task_id {
            if active.task_id != want {
                return Err(ExecutorError::Other("task_id mismatch".into()));
            }
        }
        active.cancel.cancel();
        let _ = kill_process_tree(active.pid);
        let join = active.join.take();
        drop(guard);
        if let Some(j) = join {
            let _ = j.await;
        }
        self.current.lock().await.take();
        clear_pid(self.workspace.root());
        self.load()
    }

    pub async fn wait(&self) -> Result<BridgeState, ExecutorError> {
        let join = {
            let mut guard = self.current.lock().await;
            guard.as_mut().and_then(|t| t.join.take())
        };
        if let Some(j) = join {
            let _ = j.await;
        }
        self.current.lock().await.take();
        self.load()
    }

    fn spawn_waiter(
        &self,
        child: tokio::process::Child,
        pid: u32,
        task_id: String,
        plan: C2cPlan,
        cancel: CancellationToken,
    ) -> tokio::task::JoinHandle<()> {
        let storage = self.storage.clone();
        let workspace = self.workspace.clone();
        let project_name = self.project_name.clone();
        let current = self.current.clone();
        let mode = self.mode;
        tokio::spawn(async move {
            let outcome = run_spawned(child, cancel, mode).await;
            if process_is_alive(pid) {
                let _ = kill_process_tree(pid);
            }
            clear_pid(workspace.root());
            if let Err(err) = record_outcome(
                &storage,
                &project_name,
                workspace.root(),
                &task_id,
                &plan,
                outcome,
            ) {
                tracing::error!("failed to record executor outcome: {err}");
            }
            let mut guard = current.lock().await;
            if guard.as_ref().is_some_and(|t| t.task_id == task_id) {
                *guard = None;
            }
        })
    }

    fn fail_if_running(&self, guard: &Option<ActiveTask>) -> Result<(), ExecutorError> {
        if let Some(active) = guard {
            if process_is_alive(active.pid) {
                return Err(ExecutorError::AlreadyRunning(active.task_id.clone()));
            }
        }
        if let Some(pid) = read_pid(self.workspace.root()) {
            if process_is_alive(pid) {
                return Err(ExecutorError::AlreadyRunning("unknown".into()));
            }
            clear_pid(self.workspace.root());
        }
        Ok(())
    }

    async fn reap_if_finished(&self) {
        let mut guard = self.current.lock().await;
        if let Some(active) = guard.as_ref() {
            if !process_is_alive(active.pid) {
                if let Some(join) = guard.as_mut().and_then(|t| t.join.take()) {
                    drop(guard);
                    let _ = join.await;
                    self.current.lock().await.take();
                    return;
                }
                *guard = None;
            }
        }
    }

    fn load(&self) -> Result<BridgeState, ExecutorError> {
        self.storage
            .load_task_state(&self.project_name)
            .map_err(|e| ExecutorError::Other(e.to_string()))
    }

    fn persist(&self, state: &BridgeState) -> Result<(), ExecutorError> {
        self.storage
            .save_task_state(&self.project_name, state)
            .map_err(|e| ExecutorError::Other(e.to_string()))?;
        state
            .write_c2c(
                self.workspace.root(),
                notes_for(state.task_status.unwrap_or(TaskStatus::Created)),
            )
            .map_err(|e| ExecutorError::Other(e.to_string()))?;
        Ok(())
    }

    fn mark_failed(&self, state: &mut BridgeState, err: &ExecutorError) {
        let now = Utc::now();
        state.state = C2cState::Executed;
        state.task_status = Some(TaskStatus::Failed);
        state.status = Some("failed".into());
        state.finished_at = Some(now);
        state.error = Some(err.to_string());
        state.summary = Some(err.to_string());
        state.updated_at = now;
    }
}

#[derive(Debug, Clone, Default)]
pub struct PlanInput {
    pub actions: Vec<String>,
    pub tests: Vec<String>,
    pub success_criteria: String,
}

fn next_identity(prev: &BridgeState) -> (String, u32) {
    match prev.task_status {
        Some(TaskStatus::Running) => (
            prev.task_id.clone().unwrap_or_else(new_task_id),
            prev.iteration,
        ),
        Some(TaskStatus::Done) | Some(TaskStatus::Cancelled) | None => (new_task_id(), 1),
        Some(_) if prev.task_id.is_some() => {
            (prev.task_id.clone().unwrap(), prev.iteration.max(1) + 1)
        }
        _ => (new_task_id(), 1),
    }
}

fn notes_for(status: TaskStatus) -> Option<&'static str> {
    match status {
        TaskStatus::Planned | TaskStatus::Created => Some("Executor: implement this PLAN in the workspace, run TESTS, then stop. Do not paste source."),
        TaskStatus::Running => Some("Executor is running. Poll task_status; inspect the workspace through MCP when it finishes."),
        TaskStatus::Executed | TaskStatus::Failed => Some("Please inspect git_diff, test_status, and execution_summary through MCP."),
        TaskStatus::Cancelled => Some("Executor was cancelled."),
        _ => None,
    }
}

fn record_outcome(
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
    Ok(())
}

fn mark_cancelled(state: &mut BridgeState) {
    let now = Utc::now();
    state.state = C2cState::Cancelled;
    state.task_status = Some(TaskStatus::Cancelled);
    state.status = Some("cancelled".into());
    state.finished_at = Some(now);
    state.summary = Some("Executor was cancelled.".into());
    state.error = None;
    state.updated_at = now;
}

fn finalize_tests(plan: &C2cPlan, outcome: &ExecutorOutcome) -> Option<TestResult> {
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

fn collect_changed_files(workspace: &Path) -> Vec<String> {
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

fn write_pid(workspace: &Path, pid: u32) -> Result<(), ExecutorError> {
    let dir = config::state_dir(workspace);
    fs::create_dir_all(&dir).map_err(|e| ExecutorError::Other(e.to_string()))?;
    fs::write(config::executor_pid_path(workspace), pid.to_string())
        .map_err(|e| ExecutorError::Other(e.to_string()))
}

fn read_pid(workspace: &Path) -> Option<u32> {
    fs::read_to_string(config::executor_pid_path(workspace))
        .ok()?
        .trim()
        .parse()
        .ok()
}
fn clear_pid(workspace: &Path) {
    let _ = fs::remove_file(config::executor_pid_path(workspace));
}
async fn wait_until_dead(pid: u32) {
    for _ in 0..20 {
        if !process_is_alive(pid) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}
