// src/api/system.rs
use axum::{extract::State, Json};
use serde_json::Value;

use crate::api::{bad_request, internal_error, ApiState};
use crate::config::{self, ConfigPatch, ProxyConfig, ProxyPatch};
use crate::dashboard;
use crate::executor;
use crate::models::{ConfigInput, ConnectionData, DashboardData, ProxyData, ProxyInput};

pub async fn get_dashboard(State(state): State<ApiState>) -> Result<Json<DashboardData>, (axum::http::StatusCode, String)> {
    let clients = state.oauth.list_connected_clients();
    let gateway = dashboard::gateway_status(&state.config, true, clients);
    let snapshot = dashboard::snapshot(&state.config, &state.hub, gateway).map_err(internal_error)?;
    Ok(Json(DashboardData::from(snapshot)))
}

pub async fn get_connection(State(state): State<ApiState>) -> Json<ConnectionData> {
    let path = config::find_config(None).map(|(_, p)| p).unwrap_or_default();
    Json(ConnectionData::new(&state.config, path))
}

pub async fn save_connection(Json(input): Json<ConfigInput>) -> Result<Json<ConnectionData>, (axum::http::StatusCode, String)> {
    let (mut cfg, path) = config::find_config(None).map_err(internal_error)?;
    config::patch_config(
        &mut cfg,
        ConfigPatch {
            host: Some(input.host), port: Some(input.port), no_auth: Some(input.no_auth),
            auth_token: input.auth_token, admin_password: input.admin_password,
            executor_command: Some(input.executor_command), executor_mode: Some(input.executor_mode),
            ..Default::default()
        },
    ).map_err(internal_error)?;
    cfg.save_to_path(&path).map_err(internal_error)?;
    Ok(Json(ConnectionData::new(&cfg, path)))
}

pub async fn get_settings() -> Json<Value> {
    let prefs = config::load_ui_prefs();
    Json(serde_json::json!({
        "auto_start": prefs.auto_start, "start_tunnel": prefs.start_tunnel, "notifications": false
    }))
}

pub async fn get_proxy() -> Result<Json<ProxyData>, (axum::http::StatusCode, String)> {
    let (cfg, _) = config::find_config(None).map_err(internal_error)?;
    Ok(Json(ProxyData::from(&cfg.proxy)))
}

pub async fn save_proxy(Json(input): Json<ProxyInput>) -> Result<Json<ProxyData>, (axum::http::StatusCode, String)> {
    let (mut cfg, path) = config::find_config(None).map_err(internal_error)?;
    cfg.proxy = config::apply_proxy_patch(&cfg.proxy, ProxyPatch {
        enabled: Some(input.enabled), kind: Some(input.kind), host: Some(input.host),
        port: Some(input.port), username: input.username, password: input.password,
    }).map_err(internal_error)?;
    cfg.save_to_path(&path).map_err(internal_error)?;
    Ok(Json(ProxyData::from(&cfg.proxy)))
}

pub async fn test_proxy(Json(input): Json<ProxyInput>) -> Result<String, (axum::http::StatusCode, String)> {
    let proxy = ProxyConfig {
        enabled: true, kind: input.kind, host: input.host.trim().into(), port: input.port,
        username: input.username.filter(|v| !v.trim().is_empty()),
        password: input.password.filter(|v| !v.trim().is_empty()),
    };
    executor::test_proxy(&proxy).await.map_err(bad_request)
}