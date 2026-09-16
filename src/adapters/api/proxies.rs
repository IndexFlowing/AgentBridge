// src/adapters/api/proxies.rs
//! Web REST controller for multi-proxy management (CRUD + connectivity verify).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use crate::api::{internal_error, ApiState};
use crate::config::ProxyServiceError;
use crate::models::{ProxyData, ProxyVerifyResult, SaveProxyRequest};

pub(crate) fn map_proxy_err(err: ProxyServiceError) -> (StatusCode, String) {
    match err {
        ProxyServiceError::NotFound(id) => (StatusCode::NOT_FOUND, format!("代理不存在: {id}")),
        ProxyServiceError::InUse { executors, .. } => (StatusCode::CONFLICT, executors),
        ProxyServiceError::Invalid(msg) => (StatusCode::BAD_REQUEST, msg),
        ProxyServiceError::Executor(e) => (StatusCode::BAD_REQUEST, e.to_string()),
        ProxyServiceError::Storage(e) => internal_error(e),
    }
}

#[derive(Debug, Deserialize)]
pub struct ProxyEnabledInput {
    pub enabled: bool,
}

pub async fn list_proxies(
    State(state): State<ApiState>,
) -> Result<Json<Vec<ProxyData>>, (StatusCode, String)> {
    state.proxies.list().map(Json).map_err(map_proxy_err)
}

pub async fn create_proxy(
    State(state): State<ApiState>,
    Json(input): Json<SaveProxyRequest>,
) -> Result<Json<Vec<ProxyData>>, (StatusCode, String)> {
    state.proxies.create(input).map(Json).map_err(map_proxy_err)
}

pub async fn update_proxy(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Json(input): Json<SaveProxyRequest>,
) -> Result<Json<Vec<ProxyData>>, (StatusCode, String)> {
    state
        .proxies
        .update(&id, input)
        .map(Json)
        .map_err(map_proxy_err)
}

pub async fn delete_proxy(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<ProxyData>>, (StatusCode, String)> {
    state.proxies.delete(&id).map(Json).map_err(map_proxy_err)
}

pub async fn set_proxy_enabled(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Json(input): Json<ProxyEnabledInput>,
) -> Result<Json<Vec<ProxyData>>, (StatusCode, String)> {
    state
        .proxies
        .set_enabled(&id, input.enabled)
        .map(Json)
        .map_err(map_proxy_err)
}

pub async fn verify_proxy(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<ProxyVerifyResult>, (StatusCode, String)> {
    state
        .proxies
        .verify_id(&id)
        .await
        .map(Json)
        .map_err(map_proxy_err)
}
