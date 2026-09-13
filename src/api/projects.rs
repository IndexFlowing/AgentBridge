// src/api/projects.rs
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::path::PathBuf;

use crate::api::{bad_request, internal_error, ApiState};
use crate::models::ProjectInput;
use crate::projects::{ProjectEntry, ProjectListing};
use crate::workspace::Workspace;

fn to_listing(entry: &ProjectEntry, active_name: &str) -> ProjectListing {
    let (git_repository, project_type) = if entry.path.is_dir() {
        if let Ok(ws) = Workspace::open(&entry.path, 1_048_576, false) {
            let info = ws.info();
            (info.git_repository, info.project_type)
        } else {
            (false, vec![])
        }
    } else {
        (false, vec![])
    };

    ProjectListing {
        id: entry.id.clone(),
        name: entry.name.clone(),
        path: entry.path.to_string_lossy().to_string(),
        description: entry.description.clone(),
        readonly: entry.readonly,
        active: entry.name == active_name,
        git_repository,
        project_type,
        executor: entry.executor.clone(),
    }
}

pub async fn list_projects(
    State(state): State<ApiState>,
) -> Result<Json<Vec<ProjectListing>>, (StatusCode, String)> {
    let projects = state.storage.load_projects().map_err(internal_error)?;
    let active = state.hub.default_name();
    let list = projects.iter().map(|p| to_listing(p, &active)).collect();
    Ok(Json(list))
}

pub async fn save_project(
    State(state): State<ApiState>,
    Json(input): Json<ProjectInput>,
) -> Result<Json<Vec<ProjectListing>>, (StatusCode, String)> {
    let executor = input.executor.trim().to_ascii_lowercase();
    if executor.is_empty() {
        return Err(bad_request("executor is required"));
    }

    // 【安全拦截】：若项目有正在运行的任务，禁止修改配置导致句柄孤立
    if let Some(existing) = state.hub.get(&input.name) {
        if existing.runtime.is_running().await {
            return Err((
                StatusCode::CONFLICT,
                format!("项目 `{}` 存在正在执行的任务，禁止修改配置", input.name),
            ));
        }
    }
    if let Some(ref id) = input.id {
        if let Ok(projects) = state.storage.load_projects() {
            if let Some(old_p) = projects.iter().find(|p| &p.id == id) {
                if old_p.name != input.name {
                    if let Some(existing) = state.hub.get(&old_p.name) {
                        if existing.runtime.is_running().await {
                            return Err((
                                StatusCode::CONFLICT,
                                format!("项目 `{}` 存在正在执行的任务，禁止修改配置", old_p.name),
                            ));
                        }
                    }
                }
            }
        }
    }

    let entry = ProjectEntry {
        id: input.id.unwrap_or_default(),
        name: input.name,
        path: PathBuf::from(input.path.trim()),
        description: String::new(),
        readonly: false,
        executor,
    };

    // 1. 写入 SQLite
    state
        .storage
        .upsert_project(entry)
        .map_err(internal_error)?;

    // 2. 强制通知 Hub 重建项目状态
    let _ = state.hub.reload();

    // 3. 返回最新列表
    list_projects(State(state)).await
}

pub async fn delete_project(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<ProjectListing>>, (StatusCode, String)> {
    // 【安全拦截】：若项目有正在运行的任务，禁止删除
    if let Ok(projects) = state.storage.load_projects() {
        if let Some(target) = projects.iter().find(|p| p.id == id) {
            if let Some(existing) = state.hub.get(&target.name) {
                if existing.runtime.is_running().await {
                    return Err((
                        StatusCode::CONFLICT,
                        format!("项目 `{}` 存在正在执行的任务，禁止删除", target.name),
                    ));
                }
            }
        }
    }

    state.storage.delete_project(&id).map_err(internal_error)?;
    let _ = state.hub.reload();
    list_projects(State(state)).await
}