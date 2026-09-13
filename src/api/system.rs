// src/api/system.rs
use axum::{extract::State, Json};
use serde_json::Value;

use crate::api::{internal_error, ApiState};
use crate::config::{self, Config, ProxyConfig};
use crate::dashboard;
use crate::models::{ConfigInput, ConnectionData, DashboardData, ProxyData, ProxyInput};
use crate::storage::proxies::ProxyDefinition;

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
/// only after the service restarts (`agentbridge restart`).
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

pub async fn get_proxy(
    State(state): State<ApiState>,
) -> Result<Json<ProxyData>, (axum::http::StatusCode, String)> {
    let proxies = state.storage.load_proxies().map_err(internal_error)?;
    let active = proxies
        .into_iter()
        .find(|p| p.is_default)
        .or_else(|| state.storage.load_proxies().ok()?.into_iter().next());

    match active {
        Some(p) => Ok(Json(ProxyData::from(&p.to_config()))),
        None => Ok(Json(ProxyData::from(&ProxyConfig::default()))),
    }
}

pub async fn save_proxy(
    State(state): State<ApiState>,
    Json(input): Json<ProxyInput>,
) -> Result<Json<ProxyData>, (axum::http::StatusCode, String)> {
    // 1. 读取当前数据库中已有的默认代理配置（若存在）
    let proxies = state.storage.load_proxies().map_err(internal_error)?;
    let current_def = proxies
        .into_iter()
        .find(|p| p.id == "default" || p.is_default);

    let current_cfg = current_def
        .as_ref()
        .map(|d| d.to_config())
        .unwrap_or_default();

    // 2. 构造 ProxyPatch 并复用已有的 apply_proxy_patch (内部调用 keep_secret)
    let patch = crate::config::ProxyPatch {
        enabled: Some(input.enabled),
        kind: Some(input.kind),
        host: Some(input.host),
        port: Some(input.port),
        username: input.username,
        password: input.password,
    };
    let updated_cfg =
        crate::config::apply_proxy_patch(&current_cfg, patch).map_err(internal_error)?;

    // 3. 构造落库用的 ProxyDefinition，保留已有标识并写入安全合并后的凭据
    let def = ProxyDefinition {
        id: current_def
            .as_ref()
            .map(|d| d.id.clone())
            .unwrap_or_else(|| "default".to_string()),
        name: current_def
            .as_ref()
            .map(|d| d.name.clone())
            .unwrap_or_else(|| "Default Proxy".to_string()),
        kind: updated_cfg.kind,
        host: updated_cfg.host,
        port: updated_cfg.port,
        username: updated_cfg.username,
        password: updated_cfg.password,
        enabled: updated_cfg.enabled,
        is_default: true,
    };

    // 4. 真实落库 SQLite
    state.storage.upsert_proxy(def).map_err(internal_error)?;

    // 5. 实时刷新内存中注册表，确保下一次 task_start 立即感知
    state.hub.reload_executors().map_err(internal_error)?;

    get_proxy(State(state)).await
}

pub async fn test_proxy(
    Json(input): Json<ProxyInput>,
) -> Result<String, (axum::http::StatusCode, String)> {
    let cfg = ProxyConfig {
        enabled: true,
        kind: input.kind,
        host: input.host.trim().to_string(),
        port: input.port,
        username: input.username.filter(|u| !u.trim().is_empty()),
        password: input.password.filter(|p| !p.trim().is_empty()),
    };

    // 真正发起网络连通性探测（不再是硬编码 Mock）
    crate::executor::proxy::test_proxy(&cfg)
        .await
        .map_err(internal_error)
}
