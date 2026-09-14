// src/api/providers.rs
//! Web REST controller for Providers (delegates to `ProviderService`).

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};

use crate::api::{bad_request, internal_error, ApiState};
use crate::models::{
    CredentialInput, ModelData, ModelInput, ProviderData, ProviderInput, ProviderResolveData,
    ResolveQuery,
};
use crate::provider::ProviderServiceError;

fn map_provider_err(err: ProviderServiceError) -> (StatusCode, String) {
    match err {
        ProviderServiceError::InvalidInput(msg) => bad_request(msg),
        ProviderServiceError::NotFound(id) => bad_request(format!("provider not found: {id}")),
        ProviderServiceError::Storage(e) => internal_error(e),
    }
}

pub async fn list_providers(
    State(state): State<ApiState>,
) -> Result<Json<Vec<ProviderData>>, (StatusCode, String)> {
    state
        .provider_service
        .list()
        .map(Json)
        .map_err(map_provider_err)
}

pub async fn save_provider(
    State(state): State<ApiState>,
    Json(input): Json<ProviderInput>,
) -> Result<Json<Vec<ProviderData>>, (StatusCode, String)> {
    state
        .provider_service
        .save(input)
        .map(Json)
        .map_err(map_provider_err)
}

pub async fn delete_provider(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<ProviderData>>, (StatusCode, String)> {
    state
        .provider_service
        .delete(&id)
        .map(Json)
        .map_err(map_provider_err)
}

pub async fn save_credential(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Json(input): Json<CredentialInput>,
) -> Result<Json<ProviderData>, (StatusCode, String)> {
    state
        .provider_service
        .save_credential(&id, input)
        .map(Json)
        .map_err(map_provider_err)
}

pub async fn delete_credential(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<ProviderData>, (StatusCode, String)> {
    state
        .provider_service
        .delete_credential(&id)
        .map(Json)
        .map_err(map_provider_err)
}

pub async fn list_models(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<ModelData>>, (StatusCode, String)> {
    state
        .provider_service
        .list_models(&id)
        .map(Json)
        .map_err(map_provider_err)
}

pub async fn save_model(
    State(state): State<ApiState>,
    Path(provider_id): Path<String>,
    Json(input): Json<ModelInput>,
) -> Result<Json<Vec<ModelData>>, (StatusCode, String)> {
    state
        .provider_service
        .save_model(&provider_id, input)
        .map(Json)
        .map_err(map_provider_err)
}

pub async fn delete_model(
    State(state): State<ApiState>,
    Path((provider_id, model_id)): Path<(String, String)>,
) -> Result<Json<Vec<ModelData>>, (StatusCode, String)> {
    state
        .provider_service
        .delete_model(&provider_id, &model_id)
        .map(Json)
        .map_err(map_provider_err)
}

pub async fn resolve_provider(
    State(state): State<ApiState>,
    Query(query): Query<ResolveQuery>,
) -> Result<Json<ProviderResolveData>, (StatusCode, String)> {
    state
        .provider_service
        .resolve(&query)
        .map(Json)
        .map_err(map_provider_err)
}
