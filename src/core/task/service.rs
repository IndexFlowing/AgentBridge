// src/task/service.rs
//! Application Service for Task orchestration, continuation, and query.
//!
//! Web API, MCP, and CLI must dispatch task operations through this service.

use std::sync::Arc;
use thiserror::Error;

use crate::models::{CancelTaskRequest, StartTaskRequest};
use crate::projects::ProjectHub;
use crate::state::BridgeState;
use crate::storage::Storage;

#[derive(Debug, Error)]
pub enum TaskServiceError {
    #[error("Project '{0}' not found")]
    ProjectNotFound(String),
    #[error("Project '{0}' is read-only")]
    ProjectReadonly(String),
    #[error(transparent)]
    Executor(#[from] crate::executor::ExecutorError),
    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}

#[derive(Clone)]
pub struct TaskService {
    hub: Arc<ProjectHub>,
    _storage: Arc<Storage>,
}

impl TaskService {
    pub fn new(hub: Arc<ProjectHub>, storage: Arc<Storage>) -> Self {
        Self {
            hub,
            _storage: storage,
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
}
