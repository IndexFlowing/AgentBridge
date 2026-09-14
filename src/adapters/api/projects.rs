// src/adapters/api/projects.rs
//! Web REST controller for Projects (Strictly Thin < 25 lines).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};

use crate::api::{internal_error, ApiState};
use crate::models::ProjectInput;
use crate::projects::{ProjectListing, ProjectServiceError};

pub async fn list_projects(
    State(state): State<ApiState>,
) -> Result<Json<Vec<ProjectListing>>, (StatusCode, String)> {
    state.projects.list().map(Json).map_err(internal_error)
}

pub async fn save_project(
    State(state): State<ApiState>,
    Json(input): Json<ProjectInput>,
) -> Result<Json<Vec<ProjectListing>>, (StatusCode, String)> {
    state
        .projects
        .save(input.into())
        .await
        .map(Json)
        .map_err(map_project_err)
}

pub async fn delete_project(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<ProjectListing>>, (StatusCode, String)> {
    state
        .projects
        .delete(&id)
        .await
        .map(Json)
        .map_err(map_project_err)
}

fn map_project_err(err: ProjectServiceError) -> (StatusCode, String) {
    match err {
        ProjectServiceError::TaskRunning(name) => (
            StatusCode::CONFLICT,
            format!("项目 `{name}` 存在正在执行的任务，禁止修改配置"),
        ),
        ProjectServiceError::ExecutorRequired => {
            (StatusCode::BAD_REQUEST, "executor is required".into())
        }
        ProjectServiceError::NotFound(name) => {
            (StatusCode::NOT_FOUND, format!("project `{name}` not found"))
        }
        ProjectServiceError::Storage(e) => internal_error(e),
    }
}
