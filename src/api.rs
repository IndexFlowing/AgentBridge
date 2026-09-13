// src/api.rs
use axum::{
    extract::Request,
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Router,
};
use std::sync::Arc;

pub mod executors;
pub mod projects;
pub mod system;
pub mod tasks;

use crate::config::Config;
use crate::oauth::OauthServer;
use crate::projects::ProjectHub;
use crate::storage::Storage;

#[derive(Clone)]
pub struct ApiState {
    pub config: Arc<Config>,
    pub hub: Arc<ProjectHub>,
    pub oauth: Arc<OauthServer>,
    pub storage: Arc<Storage>,
}

pub fn router(state: ApiState) -> Router {
    Router::new()
        // System
        .route("/system/dashboard", get(system::get_dashboard))
        .route("/system/connection", get(system::get_connection))
        .route("/system/connection", put(system::save_connection))
        .route("/system/settings", get(system::get_settings))
        // Proxy
        .route("/proxy", get(system::get_proxy))
        .route("/proxy", put(system::save_proxy))
        .route("/proxy/test", post(system::test_proxy))
        // Projects: 支持 GET 读取项目列表！
        .route(
            "/projects",
            get(projects::list_projects).post(projects::save_project),
        )
        .route("/projects/{id}", delete(projects::delete_project))
        // Executors
        .route("/executors", get(executors::list_executors))
        .route("/executors", post(executors::save_executor))
        .route("/executors/available", get(executors::available_executors))
        .route("/executors/{id}", delete(executors::delete_executor))
        .route("/executors/{id}/test", post(executors::test_executor))
        // Tasks
        .route("/projects/{name}/tasks/cancel", post(tasks::cancel_task))
        .with_state(state)
        .layer(middleware::from_fn(loopback_only))
}

async fn loopback_only(req: Request, next: Next) -> Response {
    let host = req
        .headers()
        .get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let is_local =
        host.starts_with("127.0.0.1") || host.starts_with("localhost") || host.starts_with("[::1]");

    if !is_local {
        tracing::warn!("Blocked non-loopback management API request: {}", host);
        return (StatusCode::FORBIDDEN, "API restricted to localhost").into_response();
    }
    next.run(req).await
}

pub(crate) fn internal_error<E: std::fmt::Display>(err: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}

pub(crate) fn bad_request<E: std::fmt::Display>(err: E) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, err.to_string())
}
