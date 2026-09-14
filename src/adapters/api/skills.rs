// src/adapters/api/skills.rs
//! Web REST controller for Skills (Strictly Thin < 25 lines per handler).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};

use crate::api::{internal_error, ApiState};
use crate::models::{InstallSkillRequest, SkillData, SkillDetailData};

pub async fn list_skills(
    State(state): State<ApiState>,
) -> Result<Json<Vec<SkillData>>, (StatusCode, String)> {
    state.skills.list_skills().map(Json).map_err(internal_error)
}

pub async fn get_skill(
    State(state): State<ApiState>,
    Path(name): Path<String>,
) -> Result<Json<SkillDetailData>, (StatusCode, String)> {
    state.skills.get_skill_detail(&name).map(Json).map_err(internal_error)
}

pub async fn install_skill(
    State(state): State<ApiState>,
    Json(req): Json<InstallSkillRequest>,
) -> Result<Json<SkillData>, (StatusCode, String)> {
    state.skills.install_skill(req).map(Json).map_err(internal_error)
}

pub async fn enable_skill(
    State(state): State<ApiState>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    state.skills.set_enabled(&name, true).map(|_| StatusCode::OK).map_err(internal_error)
}

pub async fn disable_skill(
    State(state): State<ApiState>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    state.skills.set_enabled(&name, false).map(|_| StatusCode::OK).map_err(internal_error)
}

pub async fn remove_skill(
    State(state): State<ApiState>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    state.skills.remove_skill(&name).map(|_| StatusCode::OK).map_err(internal_error)
}