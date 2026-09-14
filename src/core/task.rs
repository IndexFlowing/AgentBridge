// src/task.rs
//! TaskRuntime: Process supervision and single-project execution lifecycle.

pub mod outcome;
pub mod service;
pub mod supervisor;

pub use service::{TaskService, TaskServiceError};

use std::sync::Arc;
use chrono::Utc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::config::ExecutorMode;
use crate::executor::{
    kill_process_tree, process_is_alive, run_spawned, ExecutorError,
    SharedExecutorRegistry,
};
use crate::models::StartTaskRequest;
use crate::protocol::{C2cPlan, C2cState};
use crate::state::{new_task_id, BridgeState, TaskStatus};
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

pub struct ActiveTask {
    pub task_id: String,
    pub pid: u32,
    pub cancel: CancellationToken,
    pub join: Option<tokio::task::JoinHandle<()>>,
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

    pub async fn is_running(&self) -> bool {
        let guard = self.current.lock().await;
        if let Some(active) = guard.as_ref() {
            if process_is_alive(active.pid) {
                return true;
            }
        }
        if let Some(pid) = supervisor::read_pid(self.workspace.root()) {
            if process_is_alive(pid) {
                return true;
            }
        }
        false
    }

    /// Start a new task or explicitly continue an existing task via StartTaskRequest.
    pub async fn start_task(
        &self,
        req: StartTaskRequest,
    ) -> Result<BridgeState, ExecutorError> {
        let executor_id = req.executor.as_deref().unwrap_or(&self.default_executor);
        let registry = self.registry.read().unwrap().clone();
        let executor = registry
            .get(executor_id)
            .ok_or_else(|| ExecutorError::NotFound(executor_id.to_string()))?;
        let proxy = registry.resolve_proxy_for(executor_id);

        executor.detect()?;
        let mut guard = self.current.lock().await;
        supervisor::fail_if_running(&guard, self.workspace.root())?;

        let mut state = self.load()?;
        let (task_id, iteration) = resolve_task_identity(&state, req.continue_task_id.as_deref())?;
        let plan = req.into_c2c_plan(task_id.clone(), iteration)
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
        supervisor::write_pid(self.workspace.root(), spawned.pid)?;

        let cancel = CancellationToken::new();
        let join = self.spawn_waiter(
            spawned.child,
            spawned.pid,
            task_id.clone(),
            plan.clone(),
            cancel.clone(),
        );
        *guard = Some(ActiveTask {
            task_id,
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
            if let Some(pid) = supervisor::read_pid(self.workspace.root()) {
                if process_is_alive(pid) {
                    let _ = kill_process_tree(pid);
                    let _ = supervisor::wait_until_dead(pid).await;
                }
                supervisor::clear_pid(self.workspace.root());
                let mut state = self.load()?;
                outcome::mark_cancelled(&mut state);
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
        supervisor::clear_pid(self.workspace.root());
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
            let res = run_spawned(child, cancel, mode).await;
            if process_is_alive(pid) {
                let _ = kill_process_tree(pid);
            }
            supervisor::clear_pid(workspace.root());
            if let Err(err) = outcome::record_outcome(
                &storage,
                &project_name,
                workspace.root(),
                &task_id,
                &plan,
                res,
            ) {
                tracing::error!("failed to record executor outcome: {err}");
            }
            let mut guard = current.lock().await;
            if guard.as_ref().is_some_and(|t| t.task_id == task_id) {
                *guard = None;
            }
        })
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
                outcome::notes_for(state.task_status.unwrap_or(TaskStatus::Created)),
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

/// 确立显式迭代规则：None => 全新任务；Some(id) => 继承迭代
pub fn resolve_task_identity(
    prev: &BridgeState,
    continue_task_id: Option<&str>,
) -> Result<(String, u32), ExecutorError> {
    match continue_task_id {
        None => Ok((new_task_id(), 1)),
        Some(raw_id) => {
            let id = raw_id.trim();
            if id.is_empty() {
                return Ok((new_task_id(), 1));
            }
            let prev_id = prev.task_id.as_deref().unwrap_or("");
            if prev_id != id {
                return Err(ExecutorError::Other(format!(
                    "cannot continue task '{id}': task not found for this project (current is '{}')",
                    if prev_id.is_empty() { "(none)" } else { prev_id }
                )));
            }
            let next_iter = prev.iteration.max(1) + 1;
            Ok((id.to_string(), next_iter))
        }
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PlanInput {
    pub actions: Vec<String>,
    pub tests: Vec<String>,
    pub success_criteria: String,
}