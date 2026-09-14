// src/task/service.rs
//! Application Service for Task orchestration, continuation, and query.
//!
//! Web API, MCP, and CLI must dispatch task operations through this service.

use std::sync::Arc;
use thiserror::Error;

use chrono::Utc;

use crate::git;
use crate::models::{CancelTaskRequest, StartTaskRequest};
use crate::projects::{ProjectHandle, ProjectHub};
use crate::protocol::{C2cPlan, C2cState};
use crate::state::{new_task_id, BridgeState, TaskStatus, TestResult};
use crate::storage::Storage;

#[derive(Debug, Error)]
pub enum TaskServiceError {
    #[error("Project '{0}' not found")]
    ProjectNotFound(String),
    #[error("Project '{0}' is read-only")]
    ProjectReadonly(String),
    #[error("invalid task plan: {0}")]
    Protocol(String),
    #[error(transparent)]
    Executor(#[from] crate::executor::ExecutorError),
    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}

/// Domain values needed to record a finished executor run.
#[derive(Debug, Clone)]
pub struct ExecutedInput {
    pub task_id: Option<String>,
    pub iteration: Option<u32>,
    pub c2c_state: C2cState,
    pub task_status: TaskStatus,
    pub status: String,
    pub exit_code: Option<i32>,
    pub changed_files: Option<Vec<String>>,
    pub tests_command: Option<String>,
    pub test_summary: Option<String>,
}

#[derive(Clone)]
pub struct TaskService {
    hub: Arc<ProjectHub>,
    storage: Arc<Storage>,
}

impl TaskService {
    pub fn new(hub: Arc<ProjectHub>, storage: Arc<Storage>) -> Self {
        Self { hub, storage }
    }

    fn project(&self, name: &str) -> Result<ProjectHandle, TaskServiceError> {
        self.hub
            .get(name)
            .ok_or_else(|| TaskServiceError::ProjectNotFound(name.to_string()))
    }

    /// Read the persisted task state for a project through the Storage port.
    pub fn task_state(&self, project_name: &str) -> Result<BridgeState, TaskServiceError> {
        Ok(self.storage.load_task_state(project_name)?)
    }

    /// Unified entry point to start a new task or explicitly continue an existing task.
    pub async fn start_task(&self, req: StartTaskRequest) -> Result<BridgeState, TaskServiceError> {
        let project = self
            .hub
            .get(&req.project_name)
            .ok_or_else(|| TaskServiceError::ProjectNotFound(req.project_name.clone()))?;

        if project.readonly {
            return Err(TaskServiceError::ProjectReadonly(req.project_name.clone()));
        }

        project
            .runtime
            .start_task(req)
            .await
            .map_err(TaskServiceError::Executor)
    }

    /// Unified entry point to cancel an ongoing task.
    pub async fn cancel_task(
        &self,
        req: CancelTaskRequest,
    ) -> Result<BridgeState, TaskServiceError> {
        let project = self
            .hub
            .get(&req.project_name)
            .ok_or_else(|| TaskServiceError::ProjectNotFound(req.project_name.clone()))?;

        project
            .runtime
            .cancel(req.task_id.as_deref())
            .await
            .map_err(TaskServiceError::Executor)
    }

    /// Unified entry point to query task status.
    pub async fn get_status(
        &self,
        project_name: &str,
        task_id: Option<&str>,
    ) -> Result<BridgeState, TaskServiceError> {
        let project = self
            .hub
            .get(project_name)
            .ok_or_else(|| TaskServiceError::ProjectNotFound(project_name.to_string()))?;

        project
            .runtime
            .status(task_id)
            .await
            .map_err(TaskServiceError::Executor)
    }

    /// Await the currently running task for a project (used by the CLI).
    pub async fn wait(&self, project_name: &str) -> Result<BridgeState, TaskServiceError> {
        let project = self.project(project_name)?;
        project
            .runtime
            .wait()
            .await
            .map_err(TaskServiceError::Executor)
    }

    /// Persist a plan-only task record and refresh the C2C handoff file.
    pub fn plan_task(
        &self,
        project_name: &str,
        goal: String,
        tests: Vec<String>,
        executor: &str,
    ) -> Result<BridgeState, TaskServiceError> {
        let project = self.project(project_name)?;
        let workspace = project.workspace.root().display().to_string();
        let mut state = self.storage.load_task_state(project_name)?;
        let plan = C2cPlan::new(
            new_task_id(),
            1,
            goal,
            vec!["Implement the goal in the workspace.".into()],
            tests,
            "Goal implemented and tests pass.".into(),
        )
        .map_err(|e| TaskServiceError::Protocol(e.to_string()))?;

        state.apply_plan(&plan, &workspace, executor);
        self.storage.save_task_state(project_name, &state)?;
        state.write_c2c(
            project.workspace.root(),
            Some("Executor: implement this PLAN, run TESTS, then report."),
        )?;
        Ok(state)
    }

    /// Record the outcome reported by an executor and refresh the C2C handoff.
    pub fn record_executed(
        &self,
        project_name: &str,
        input: ExecutedInput,
    ) -> Result<BridgeState, TaskServiceError> {
        let project = self.project(project_name)?;
        let mut state = self.storage.load_task_state(project_name)?;

        if let Some(id) = input.task_id {
            state.task_id = Some(id);
        }
        state.iteration = input.iteration.unwrap_or(state.iteration.max(1));
        state.state = input.c2c_state;
        state.task_status = Some(input.task_status);
        state.status = Some(input.status);
        state.finished_at = Some(Utc::now());
        state.exit_code = input.exit_code;

        if let Some(files) = input.changed_files {
            state.changed_files = files;
        } else if git::is_repository(project.workspace.root()) {
            if let Ok(status) = git::status(project.workspace.root()) {
                let mut files = status.changed_files;
                files.extend(status.untracked_files);
                files.sort();
                files.dedup();
                state.changed_files = files;
            }
        }

        let command = input
            .tests_command
            .or_else(|| state.tests.as_ref().map(|t| t.command.clone()));
        if let Some(command) = command {
            let passed =
                input.exit_code.unwrap_or(1) == 0 && input.task_status == TaskStatus::Executed;
            state.tests = Some(TestResult {
                status: if passed {
                    "passed".into()
                } else {
                    "failed".into()
                },
                command,
                exit_code: input.exit_code,
                summary: input.test_summary,
                timestamp: Utc::now(),
            });
        }
        state.updated_at = Utc::now();

        self.storage.save_task_state(project_name, &state)?;
        state.write_c2c(
            project.workspace.root(),
            Some("Please inspect through MCP."),
        )?;
        Ok(state)
    }
}
