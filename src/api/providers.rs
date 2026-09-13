// src/api/providers.rs
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
use crate::provider::{reload_provider_registry, ModelDefinition, ProviderDefinition};
use crate::storage::Storage;

fn provider_data(
    storage: &Storage,
    provider: &ProviderDefinition,
    models: Vec<ModelDefinition>,
) -> anyhow::Result<ProviderData> {
    let metadata = storage.provider_credential_metadata(&provider.id)?;
    Ok(ProviderData {
        id: provider.id.clone(),
        name: provider.name.clone(),
        kind: provider.kind.clone(),
        base_url: provider.base_url.clone(),
        enabled: provider.enabled,
        is_default: provider.is_default,
        proxy_id: provider.proxy_id.clone(),
        credential_configured: metadata.as_ref().is_some_and(|m| m.configured),
        credential_updated_at: metadata.and_then(|m| m.updated_at),
        models: models.into_iter().map(ModelData::from).collect(),
    })
}

pub async fn list_providers(
    State(state): State<ApiState>,
) -> Result<Json<Vec<ProviderData>>, (StatusCode, String)> {
    let providers = state.storage.load_providers().map_err(internal_error)?;
    let models = state.storage.load_models().map_err(internal_error)?;
    let mut out = Vec::new();
    for provider in providers {
        let owned: Vec<ModelDefinition> = models
            .iter()
            .filter(|m| m.provider_id == provider.id)
            .cloned()
            .collect();
        out.push(provider_data(&state.storage, &provider, owned).map_err(internal_error)?);
    }
    Ok(Json(out))
}

pub async fn save_provider(
    State(state): State<ApiState>,
    Json(input): Json<ProviderInput>,
) -> Result<Json<Vec<ProviderData>>, (StatusCode, String)> {
    let requested_id = input
        .id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let existing = match &requested_id {
        Some(id) => state.storage.get_provider(id).map_err(internal_error)?,
        None => None,
    };
    let id = requested_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let provider = ProviderDefinition {
        id: id.clone(),
        name: input.name.trim().to_string(),
        kind: input.kind.trim().to_ascii_lowercase(),
        base_url: input.base_url.unwrap_or_default().trim().to_string(),
        enabled: input.enabled.unwrap_or(true),
        is_default: input
            .is_default
            .unwrap_or_else(|| existing.as_ref().is_some_and(|p| p.is_default)),
        proxy_id: input
            .proxy_id
            .filter(|v| !v.trim().is_empty())
            .or_else(|| existing.as_ref().and_then(|p| p.proxy_id.clone())),
    };

    state
        .storage
        .upsert_provider(provider)
        .map_err(bad_request)?;

    // Secret lifecycle: omitted or empty never clears an existing secret.
    if let Some(secret) = input.api_key.as_deref() {
        if !secret.trim().is_empty() {
            state
                .storage
                .save_provider_secret(&id, secret)
                .map_err(bad_request)?;
        }
    }

    reload_provider_registry(&state.providers, &state.storage).map_err(internal_error)?;
    list_providers(State(state)).await
}

pub async fn delete_provider(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<ProviderData>>, (StatusCode, String)> {
    state.storage.delete_provider(&id).map_err(internal_error)?;
    reload_provider_registry(&state.providers, &state.storage).map_err(internal_error)?;
    list_providers(State(state)).await
}

pub async fn save_credential(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Json(input): Json<CredentialInput>,
) -> Result<Json<ProviderData>, (StatusCode, String)> {
    let secret = input
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| bad_request("api_key is required"))?;
    state
        .storage
        .save_provider_secret(&id, secret)
        .map_err(bad_request)?;

    let provider = state
        .storage
        .get_provider(&id)
        .map_err(internal_error)?
        .ok_or_else(|| bad_request("provider not found"))?;
    let models = state
        .storage
        .load_models_for_provider(&id)
        .map_err(internal_error)?;
    Ok(Json(
        provider_data(&state.storage, &provider, models).map_err(internal_error)?,
    ))
}

pub async fn delete_credential(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<ProviderData>, (StatusCode, String)> {
    state
        .storage
        .delete_provider_secret(&id)
        .map_err(internal_error)?;
    let provider = state
        .storage
        .get_provider(&id)
        .map_err(internal_error)?
        .ok_or_else(|| bad_request("provider not found"))?;
    let models = state
        .storage
        .load_models_for_provider(&id)
        .map_err(internal_error)?;
    Ok(Json(
        provider_data(&state.storage, &provider, models).map_err(internal_error)?,
    ))
}

pub async fn list_models(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<ModelData>>, (StatusCode, String)> {
    let models = state
        .storage
        .load_models_for_provider(&id)
        .map_err(internal_error)?;
    Ok(Json(models.into_iter().map(ModelData::from).collect()))
}

pub async fn save_model(
    State(state): State<ApiState>,
    Path(provider_id): Path<String>,
    Json(input): Json<ModelInput>,
) -> Result<Json<Vec<ModelData>>, (StatusCode, String)> {
    let model = ModelDefinition {
        id: input.id.unwrap_or_default(),
        provider_id: provider_id.clone(),
        name: input.name.trim().to_string(),
        enabled: input.enabled.unwrap_or(true),
    };
    state.storage.upsert_model(model).map_err(bad_request)?;
    reload_provider_registry(&state.providers, &state.storage).map_err(internal_error)?;
    list_models(State(state), Path(provider_id)).await
}

pub async fn delete_model(
    State(state): State<ApiState>,
    Path((provider_id, model_id)): Path<(String, String)>,
) -> Result<Json<Vec<ModelData>>, (StatusCode, String)> {
    state
        .storage
        .delete_model(&model_id)
        .map_err(internal_error)?;
    reload_provider_registry(&state.providers, &state.storage).map_err(internal_error)?;
    list_models(State(state), Path(provider_id)).await
}

pub async fn resolve_provider(
    State(state): State<ApiState>,
    Query(query): Query<ResolveQuery>,
) -> Result<Json<ProviderResolveData>, (StatusCode, String)> {
    let registry = state
        .providers
        .read()
        .map(|guard| guard.clone())
        .map_err(internal_error)?;

    let provider = registry
        .resolve_provider(query.provider.as_deref())
        .ok_or_else(|| bad_request("provider not found"))?;
    let model = registry.resolve_model(provider, query.model.as_deref());

    Ok(Json(ProviderResolveData {
        provider_id: provider.id.clone(),
        provider_name: provider.name.clone(),
        kind: provider.kind.clone(),
        is_default: provider.is_default,
        model_id: model.map(|m| m.id.clone()),
        model_name: model.map(|m| m.name.clone()),
    }))
}
