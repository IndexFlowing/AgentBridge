// src/adapters/api.rs
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
pub mod providers;
pub mod system;
pub mod tasks;
pub mod skills;

use crate::oauth::OauthServer;

use std::ops::Deref;
use crate::core::AppCore;

#[derive(Clone)]
pub struct ApiState {
    pub core: Arc<AppCore>,
    pub oauth: Arc<OauthServer>,
}

impl Deref for ApiState {
    type Target = AppCore;
    fn deref(&self) -> &Self::Target {
        &self.core
    }
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
        // Projects
        .route(
            "/projects",
            get(projects::list_projects).post(projects::save_project),
        )
        .route("/projects/{id}", delete(projects::delete_project))
        // Skills
        .route("/skills", get(skills::list_skills).post(skills::install_skill))
        .route("/skills/{name}", get(skills::get_skill).delete(skills::remove_skill))
        .route("/skills/{name}/enable", put(skills::enable_skill))
        .route("/skills/{name}/disable", put(skills::disable_skill))
        // Executors
        .route("/executors", get(executors::list_executors))
        .route("/executors", post(executors::save_executor))
        .route("/executors/available", get(executors::available_executors))
        .route("/executors/{id}", delete(executors::delete_executor))
        .route("/executors/{id}/test", post(executors::test_executor))
        // Providers
        .route(
            "/providers",
            get(providers::list_providers).post(providers::save_provider),
        )
        .route("/providers/resolve", get(providers::resolve_provider))
        .route("/providers/{id}", delete(providers::delete_provider))
        .route(
            "/providers/{id}/credential",
            post(providers::save_credential).delete(providers::delete_credential),
        )
        .route(
            "/providers/{id}/models",
            get(providers::list_models).post(providers::save_model),
        )
        .route(
            "/providers/{id}/models/{model_id}",
            delete(providers::delete_model),
        )
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