// src/api/executors.rs
use axum::{extract::Path, Json};
use std::path::PathBuf;

use crate::api::internal_error;
use crate::config::{self, ExecutorUpsert};
use crate::executor;
use crate::models::{ExecutorData, ExecutorInput};

pub async fn available_executors() -> Result<Json<Vec<String>>, (axum::http::StatusCode, String)> {
    let (_, path) = config::find_config(None).map_err(internal_error)?;
    let kinds = executor::available_kinds(&path).map_err(internal_error)?;
    Ok(Json(kinds))
}

pub async fn list_executors() -> Result<Json<Vec<ExecutorData>>, (axum::http::StatusCode, String)> {
    let (_, path) = config::find_config(None).map_err(internal_error)?;
    let views = executor::list_views(&path).map_err(internal_error)?;
    Ok(Json(views.into_iter().map(ExecutorData::from).collect()))
}

pub async fn save_executor(Json(input): Json<ExecutorInput>) -> Result<Json<Vec<ExecutorData>>, (axum::http::StatusCode, String)> {
    let (_, path) = config::find_config(None).map_err(internal_error)?;
    config::upsert_executor(&path, ExecutorUpsert {
        id: input.id.filter(|v| !v.trim().is_empty()),
        name: input.name,
        kind: input.kind,
        command: input.command,
        executable: input.executable.filter(|v| !v.trim().is_empty()).map(PathBuf::from),
        working_directory: input.working_directory.filter(|v| !v.trim().is_empty()).map(PathBuf::from),
        proxy_id: input.proxy_id.filter(|v| !v.trim().is_empty()),
        enabled: Some(input.enabled),
    }).map_err(internal_error)?;
    list_executors().await
}

pub async fn delete_executor(Path(id): Path<String>) -> Result<Json<Vec<ExecutorData>>, (axum::http::StatusCode, String)> {
    let (_, path) = config::find_config(None).map_err(internal_error)?;
    config::remove_executor(&path, &id).map_err(internal_error)?;
    list_executors().await
}

pub async fn test_executor(Path(id): Path<String>) -> Result<Json<ExecutorData>, (axum::http::StatusCode, String)> {
    let (_, path) = config::find_config(None).map_err(internal_error)?;
    let view = executor::list_views(&path).map_err(internal_error)?
        .into_iter().find(|v| v.definition.id == id)
        .ok_or_else(|| internal_error("executor not found"))?;
    Ok(Json(ExecutorData::from(view)))
}