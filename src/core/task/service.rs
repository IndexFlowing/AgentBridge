// src/task/service.rs
//! Application Service for Task orchestration, continuation, and query.
//!
//! Web API, MCP, and CLI must dispatch task operations through this service.

use std::sync::Arc;
use thiserror::Error;

use super::outcome::{self, OutcomeReport};
use crate::models::{CancelTaskRequest, StartTaskRequest};
use crate::projects::{ProjectHandle, ProjectHub};
use crate::protocol::{C2cPlan, C2cState};
use crate::state::{new_task_id, BridgeState, TaskStatus};
use crate::storage::Storage;

#[derive(Debug, Error)]
pub enum TaskServiceError {
    #[error("Project '{0}' not found")]
    ProjectNotFound(String),
    #[error("Task '{0}' not found in any project")]
    TaskNotFound(String),
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

    /// Read a persisted task by its globally unique `task_id`.
    ///
    /// The `task_id` is the authoritative identity; the owning project is
    /// derived from storage rather than assumed from the active project.
    pub fn task_state_by_id(&self, task_id: &str) -> Result<BridgeState, TaskServiceError> {
        let id = task_id.trim();
        if id.is_empty() {
            return Err(TaskServiceError::TaskNotFound(String::new()));
        }
        self.storage
            .find_task_by_id(id)?
            .map(|(_, state)| state)
            .ok_or_else(|| TaskServiceError::TaskNotFound(id.to_string()))
    }

    /// Resolve which project owns `task_id`, ignoring the requested project.
    ///
    /// When no `task_id` is supplied, operations stay scoped to the explicitly
    /// requested project (never to a cross-project "most recent" task).
    fn owner_project(
        &self,
        requested: &str,
        task_id: Option<&str>,
    ) -> Result<String, TaskServiceError> {
        match task_id.map(str::trim).filter(|id| !id.is_empty()) {
            Some(id) => self
                .storage
                .find_task_by_id(id)?
                .map(|(project, _)| project)
                .ok_or_else(|| TaskServiceError::TaskNotFound(id.to_string())),
            None => Ok(requested.to_string()),
        }
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
        let owner = self.owner_project(&req.project_name, req.task_id.as_deref())?;
        let project = self
            .hub
            .get(&owner)
            .ok_or_else(|| TaskServiceError::ProjectNotFound(owner.clone()))?;

        project
            .runtime
            .cancel(req.task_id.as_deref())
            .await
            .map_err(TaskServiceError::Executor)
    }

    /// Unified entry point to query task status.
    ///
    /// A supplied `task_id` selects its owning project; otherwise the query is
    /// scoped to `project_name` only.
    pub async fn get_status(
        &self,
        project_name: &str,
        task_id: Option<&str>,
    ) -> Result<BridgeState, TaskServiceError> {
        let owner = self.owner_project(project_name, task_id)?;
        let project = self
            .hub
            .get(&owner)
            .ok_or_else(|| TaskServiceError::ProjectNotFound(owner.clone()))?;

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

    /// Persist a plan-only task record into the AgentBridge state layer.
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
        Ok(state)
    }

    /// Record the outcome reported by an executor into the AgentBridge state layer.
    ///
    /// Outcome rules (status derivation, changed files, test result) are owned
    /// by [`outcome::apply`]; this method only normalizes the reported input and
    /// persists the result.
    pub fn record_executed(
        &self,
        project_name: &str,
        input: ExecutedInput,
    ) -> Result<BridgeState, TaskServiceError> {
        let project = self.project(project_name)?;
        let mut state = match input
            .task_id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
        {
            Some(id) => self
                .storage
                .load_task_state_by_id(id)?
                .unwrap_or(self.storage.load_task_state(project_name)?),
            None => self.storage.load_task_state(project_name)?,
        };

        if let Some(id) = input.task_id.clone() {
            state.task_id = Some(id);
        }
        state.iteration = input.iteration.unwrap_or(state.iteration.max(1));

        let tests_command = input
            .tests_command
            .clone()
            .or_else(|| state.tests.as_ref().map(|t| t.command.clone()));

        let report = OutcomeReport {
            task_status: input.task_status,
            c2c_state: input.c2c_state,
            exit_code: input.exit_code,
            summary: None,
            error: None,
            changed_files: input.changed_files.clone(),
            tests_command,
            tests_summary: input.test_summary.clone(),
            tests_excerpt: None,
        };
        outcome::apply(&mut state, project.workspace.root(), report);

        self.storage.save_task_state(project_name, &state)?;
        Ok(state)
    }
}
