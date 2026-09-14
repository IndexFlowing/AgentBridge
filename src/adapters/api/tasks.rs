// src/api/tasks.rs
//! Web REST controller for task operations (Strictly thin < 25 lines).

use axum::{
    extract::{Path, State},
    Json,
};
use serde::Deserialize;

use crate::api::{internal_error, ApiState};
use crate::dashboard;
use crate::models::{CancelTaskRequest, TaskData};

#[derive(Deserialize)]
pub struct CancelTaskInput {
    pub task_id: Option<String>,
}

pub async fn cancel_task(
    State(state): State<ApiState>,
    Path(project_name): Path<String>,
    Json(input): Json<CancelTaskInput>,
) -> Result<Json<TaskData>, (axum::http::StatusCode, String)> {
    let req = CancelTaskRequest {
        project_name: project_name.clone(),
        task_id: input.task_id,
    };
    state.tasks.cancel_task(req).await.map_err(internal_error)?;
    let snapshot = dashboard::task_snapshot(&project_name, &state.storage).map_err(internal_error)?;
    Ok(Json(TaskData::from(snapshot)))
}