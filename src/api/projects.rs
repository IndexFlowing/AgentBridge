// src/api/projects.rs
use axum::{extract::{Path, State}, Json};
use std::path::PathBuf;

use crate::api::{bad_request, internal_error, ApiState};
use crate::models::ProjectInput;
use crate::projects::{self, ProjectListing, ProjectUpsert};

pub async fn save_project(State(state): State<ApiState>, Json(input): Json<ProjectInput>) -> Result<Json<Vec<ProjectListing>>, (axum::http::StatusCode, String)> {
    let file = projects::projects_file_for_config(state.hub.config_path());
    let executor = input.executor.trim().to_ascii_lowercase();
    if executor.is_empty() { return Err(bad_request("executor is required")); }

    projects::upsert_project(&file, ProjectUpsert {
        id: input.id.filter(|v| !v.trim().is_empty()),
        name: input.name,
        path: PathBuf::from(input.path.trim()),
        executor: Some(executor),
        ..Default::default()
    }).map_err(internal_error)?;

    state.hub.reload().unwrap_or(false);
    Ok(Json(state.hub.list(state.hub.default_name())))
}

pub async fn delete_project(State(state): State<ApiState>, Path(id): Path<String>) -> Result<Json<Vec<ProjectListing>>, (axum::http::StatusCode, String)> {
    let file = projects::projects_file_for_config(state.hub.config_path());
    projects::remove_project(&file, &id).map_err(internal_error)?;
    state.hub.reload().unwrap_or(false);
    Ok(Json(state.hub.list(state.hub.default_name())))
}