// src/api/tasks.rs
use axum::{
    extract::{Path, State},
    Json,
};
use serde::Deserialize;

use crate::api::{bad_request, internal_error, ApiState};
use crate::dashboard;
use crate::models::TaskData;

#[derive(Deserialize)]
pub struct CancelTaskInput {
    pub task_id: Option<String>,
}

pub async fn cancel_task(
    State(state): State<ApiState>,
    Path(project_name): Path<String>,
    Json(input): Json<CancelTaskInput>,
) -> Result<Json<TaskData>, (axum::http::StatusCode, String)> {
    let project = state
        .hub
        .get(&project_name)
        .ok_or_else(|| bad_request("project not found"))?;
    project
        .runtime
        .cancel(input.task_id.as_deref())
        .await
        .map_err(internal_error)?;
    let snapshot =
        dashboard::task_snapshot(&project.name, &state.storage).map_err(internal_error)?;
    Ok(Json(TaskData::from(snapshot)))
}
