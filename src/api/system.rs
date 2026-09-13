// src/api/system.rs
use axum::{extract::State, Json};
use serde_json::Value;

use crate::api::{internal_error, ApiState};
use crate::config::{self, Config};
use crate::dashboard;
use crate::models::{ConfigInput, ConnectionData, DashboardData};

pub async fn get_dashboard(
    State(state): State<ApiState>,
) -> Result<Json<DashboardData>, (axum::http::StatusCode, String)> {
    let clients = state.oauth.list_connected_clients();
    let gateway = dashboard::gateway_status(&state.config, true, clients);
    let snapshot = dashboard::snapshot(&state.config, &state.hub, gateway, &state.storage)
        .map_err(internal_error)?;
    Ok(Json(DashboardData::from(snapshot)))
}

/// Startup config is read from `~/.agentbridge/config.toml`; edits are applied
/// only after `agentbridge serve` restarts.
pub async fn get_connection(State(state): State<ApiState>) -> Json<ConnectionData> {
    let path = config::user_config_path().unwrap_or_else(|_| config::project_config_path());
    let cfg = Config::load_or_create(&path).unwrap_or_else(|_| (*state.config).clone());
    Json(ConnectionData::new(&cfg, path))
}

pub async fn save_connection(
    Json(input): Json<ConfigInput>,
) -> Result<Json<ConnectionData>, (axum::http::StatusCode, String)> {
    let path = config::user_config_path().map_err(internal_error)?;
    let mut cfg = Config::load_or_create(&path).map_err(internal_error)?;

    if !input.host.trim().is_empty() {
        cfg.host = input.host.trim().to_string();
    }
    if input.port != 0 {
        cfg.port = input.port;
    }
    cfg.no_auth = input.no_auth;
    if let Some(allow_any_host) = input.allow_any_host {
        cfg.allow_any_host = allow_any_host;
    }
    if let Some(token) = input.auth_token {
        if !token.trim().is_empty() {
            cfg.auth_token = Some(token.trim().to_string());
        }
    }
    if let Some(pw) = input.admin_password {
        if !pw.trim().is_empty() {
            cfg.admin_password = Some(pw.trim().to_string());
        }
    }
    if !input.executor_command.trim().is_empty() {
        cfg.executor.command = input.executor_command.trim().to_string();
    }
    cfg.executor.mode = input.executor_mode;

    cfg.save_to_path(&path).map_err(internal_error)?;
    Ok(Json(ConnectionData::new(&cfg, path)))
}

pub async fn get_settings() -> Json<Value> {
    Json(serde_json::json!({ "auto_start": true, "start_tunnel": false, "notifications": false }))
}

pub async fn get_proxy() -> Result<Json<crate::models::ProxyData>, (axum::http::StatusCode, String)>
{
    Ok(Json(crate::models::ProxyData::from(
        &crate::config::ProxyConfig::default(),
    )))
}

pub async fn save_proxy(
    Json(input): Json<crate::models::ProxyInput>,
) -> Result<Json<crate::models::ProxyData>, (axum::http::StatusCode, String)> {
    Ok(Json(crate::models::ProxyData {
        enabled: input.enabled,
        kind: input.kind,
        host: input.host,
        port: input.port,
        username_configured: input.username.is_some(),
        password_configured: input.password.is_some(),
    }))
}

pub async fn test_proxy() -> Result<String, (axum::http::StatusCode, String)> {
    Ok("Proxy test mock success".into())
}
