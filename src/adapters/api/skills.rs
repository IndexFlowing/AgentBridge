// src/adapters/api/skills.rs
//! Web REST controller for Skills (Strictly Thin < 25 lines per handler).

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};

use crate::api::{internal_error, ApiState};
use crate::core::agent::AgentContext;
use crate::models::{InstallSkillRequest, SkillData, SkillDetailData};
use crate::skill::AgentView;

#[derive(Debug, serde::Deserialize)]
pub struct AgentQuery {
    #[serde(default)]
    pub project: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct AgentContextQuery {
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub task_id: Option<String>,
    /// Comma-separated requested skill names.
    #[serde(default)]
    pub skills: Option<String>,
}

/// Resolve the AgentContext a task would receive for a project. Diagnostic
/// only: nothing is executed and no workspace file is written.
pub async fn get_agent_context(
    State(state): State<ApiState>,
    Query(query): Query<AgentContextQuery>,
) -> Result<Json<AgentContext>, (StatusCode, String)> {
    let project = query
        .project
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| state.hub.default_name());
    let handle = state
        .hub
        .get(&project)
        .ok_or_else(|| project_not_found(&project))?;
    state
        .skills
        .resolve_context(
            &project,
            handle.workspace.root(),
            &clean_task_id(query.task_id),
            &split_skills(query.skills),
        )
        .map(Json)
        .map_err(internal_error)
}

fn project_not_found(name: &str) -> (StatusCode, String) {
    (StatusCode::NOT_FOUND, format!("project `{name}` not found"))
}

fn clean_task_id(task_id: Option<String>) -> String {
    task_id
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "preview".to_string())
}

fn split_skills(raw: Option<String>) -> Vec<String> {
    raw.unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

pub async fn get_agent(
    State(state): State<ApiState>,
    Query(query): Query<AgentQuery>,
) -> Result<Json<AgentView>, (StatusCode, String)> {
    state
        .skills
        .agent_view(query.project.as_deref())
        .map(Json)
        .map_err(internal_error)
}

pub async fn list_skills(
    State(state): State<ApiState>,
) -> Result<Json<Vec<SkillData>>, (StatusCode, String)> {
    state.skills.list_skills().map(Json).map_err(internal_error)
}

pub async fn get_skill(
    State(state): State<ApiState>,
    Path(name): Path<String>,
) -> Result<Json<SkillDetailData>, (StatusCode, String)> {
    state
        .skills
        .get_skill_detail(&name)
        .map(Json)
        .map_err(internal_error)
}

pub async fn install_skill(
    State(state): State<ApiState>,
    Json(req): Json<InstallSkillRequest>,
) -> Result<Json<SkillData>, (StatusCode, String)> {
    state
        .skills
        .install_skill(req)
        .map(Json)
        .map_err(internal_error)
}

pub async fn enable_skill(
    State(state): State<ApiState>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .skills
        .set_enabled(&name, true)
        .map(|_| StatusCode::OK)
        .map_err(internal_error)
}

pub async fn disable_skill(
    State(state): State<ApiState>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .skills
        .set_enabled(&name, false)
        .map(|_| StatusCode::OK)
        .map_err(internal_error)
}

pub async fn remove_skill(
    State(state): State<ApiState>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .skills
        .remove_skill(&name)
        .map(|_| StatusCode::OK)
        .map_err(internal_error)
}
