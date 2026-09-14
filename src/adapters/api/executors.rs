// src/adapters/api/executors.rs
//! Web REST controller for Executors (Strictly Thin < 25 lines).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};

use crate::api::{internal_error, ApiState};
use crate::models::{ExecutorData, ExecutorInput};

pub async fn available_executors(
    State(state): State<ApiState>,
) -> Result<Json<Vec<String>>, (StatusCode, String)> {
    Ok(Json(state.executors.available()))
}

pub async fn list_executors(
    State(state): State<ApiState>,
) -> Result<Json<Vec<ExecutorData>>, (StatusCode, String)> {
    state.executors.list().map(Json).map_err(internal_error)
}

pub async fn save_executor(
    State(state): State<ApiState>,
    Json(input): Json<ExecutorInput>,
) -> Result<Json<Vec<ExecutorData>>, (StatusCode, String)> {
    state
        .executors
        .save(input.into())
        .map(Json)
        .map_err(internal_error)
}

pub async fn delete_executor(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<ExecutorData>>, (StatusCode, String)> {
    state
        .executors
        .delete(&id)
        .map(Json)
        .map_err(internal_error)
}

pub async fn test_executor(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<ExecutorData>, (StatusCode, String)> {
    state.executors.test(&id).map(Json).map_err(internal_error)
}
