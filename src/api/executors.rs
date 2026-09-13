// src/api/executors.rs
use axum::{
    extract::{Path, State},
    Json,
};
use std::path::PathBuf;

use crate::api::{internal_error, ApiState};
use crate::config::ExecutorDefinition;
use crate::executor::{scan_executor, ExecutorView};
use crate::models::{ExecutorData, ExecutorInput};

pub async fn available_executors() -> Result<Json<Vec<String>>, (axum::http::StatusCode, String)> {
    // 固定返回可用的内置类型，不再扫描文件
    Ok(Json(vec!["opencode".to_string()]))
}

pub async fn list_executors(
    State(state): State<ApiState>,
) -> Result<Json<Vec<ExecutorData>>, (axum::http::StatusCode, String)> {
    // 从 SQLite 读取自定义执行器
    let custom_defs = state.storage.load_executors().map_err(internal_error)?;

    // 获取系统的内置基础执行器 (OpenCode, Claude Code等)
    let mut all_defs = crate::executor::common_executor_definitions();
    all_defs.extend(custom_defs);

    let mut views = Vec::new();
    for def in all_defs {
        let availability = scan_executor(&def);
        views.push(ExecutorData::from(ExecutorView {
            definition: def,
            detected: false, // 简化处理，统一视为已知
            availability,
        }));
    }
    Ok(Json(views))
}

pub async fn save_executor(
    State(state): State<ApiState>,
    Json(input): Json<ExecutorInput>,
) -> Result<Json<Vec<ExecutorData>>, (axum::http::StatusCode, String)> {
    let def = ExecutorDefinition {
        id: input.id.unwrap_or_default(),
        display_name: input.name.clone(),
        name: input.name,
        kind: input.kind,
        command: input.command,
        executable: input
            .executable
            .filter(|v| !v.trim().is_empty())
            .map(PathBuf::from),
        working_directory: input
            .working_directory
            .filter(|v| !v.trim().is_empty())
            .map(PathBuf::from),
        proxy_id: input.proxy_id.filter(|v| !v.trim().is_empty()),
        enabled: input.enabled,
    };

    // 写入 SQLite
    state.storage.upsert_executor(def).map_err(internal_error)?;

    // 让运行中的实例立即重建 ExecutorRegistry，后续任务使用最新配置
    state.hub.reload_executors().map_err(internal_error)?;

    list_executors(State(state)).await
}

pub async fn delete_executor(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<ExecutorData>>, (axum::http::StatusCode, String)> {
    // 从 SQLite 删除
    state.storage.delete_executor(&id).map_err(internal_error)?;

    // 删除后同样重建 Registry，避免运行中的任务继续解析到已删除的执行器
    state.hub.reload_executors().map_err(internal_error)?;

    list_executors(State(state)).await
}

pub async fn test_executor(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<ExecutorData>, (axum::http::StatusCode, String)> {
    let res = list_executors(State(state)).await?;
    let ex = res
        .0
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| internal_error("executor not found"))?;
    Ok(Json(ex))
}
