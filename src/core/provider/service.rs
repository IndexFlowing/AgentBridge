// src/core/provider/service.rs
//! Application Service for Provider, Model and Credential management.
//!
//! This is the business boundary for the Provider domain: adapters (Web, CLI,
//! MCP) call these methods instead of reaching into `Storage` directly. The
//! service owns validation, the encrypted-secret lifecycle, and hot-reloading
//! the in-memory [`ProviderRegistry`](crate::provider::ProviderRegistry).

use std::sync::Arc;
use thiserror::Error;

use crate::models::{
    CredentialInput, ModelData, ModelInput, ProviderData, ProviderInput, ProviderResolveData,
    ResolveQuery,
};
use crate::provider::{
    reload_provider_registry, ModelDefinition, ProviderDefinition, SharedProviderRegistry,
};
use crate::storage::Storage;

#[derive(Debug, Error)]
pub enum ProviderServiceError {
    #[error("provider not found: {0}")]
    NotFound(String),
    #[error("{0}")]
    InvalidInput(String),
    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}

#[derive(Clone)]
pub struct ProviderService {
    storage: Arc<Storage>,
    registry: SharedProviderRegistry,
}

impl ProviderService {
    pub fn new(storage: Arc<Storage>, registry: SharedProviderRegistry) -> Self {
        Self { storage, registry }
    }

    /// Rebuild the shared registry from SQLite and swap it in atomically.
    fn reload(&self) -> Result<(), ProviderServiceError> {
        reload_provider_registry(&self.registry, &self.storage)?;
        Ok(())
    }

    fn snapshot(&self) -> Result<Arc<crate::provider::ProviderRegistry>, ProviderServiceError> {
        self.registry
            .read()
            .map(|guard| guard.clone())
            .map_err(|_| {
                ProviderServiceError::Storage(anyhow::anyhow!("provider registry lock poisoned"))
            })
    }

    fn to_data(
        &self,
        provider: &ProviderDefinition,
        models: Vec<ModelDefinition>,
    ) -> Result<ProviderData, ProviderServiceError> {
        let metadata = self.storage.provider_credential_metadata(&provider.id)?;
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

    fn models_data(&self, provider_id: &str) -> Result<Vec<ModelData>, ProviderServiceError> {
        let models = self.storage.load_models_for_provider(provider_id)?;
        Ok(models.into_iter().map(ModelData::from).collect())
    }

    pub fn list(&self) -> Result<Vec<ProviderData>, ProviderServiceError> {
        let providers = self.storage.load_providers()?;
        let models = self.storage.load_models()?;
        let mut out = Vec::with_capacity(providers.len());
        for provider in providers {
            let owned: Vec<ModelDefinition> = models
                .iter()
                .filter(|m| m.provider_id == provider.id)
                .cloned()
                .collect();
            out.push(self.to_data(&provider, owned)?);
        }
        Ok(out)
    }

    pub fn save(&self, input: ProviderInput) -> Result<Vec<ProviderData>, ProviderServiceError> {
        let requested_id = input
            .id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let existing = match &requested_id {
            Some(id) => self.storage.get_provider(id)?,
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

        self.storage
            .upsert_provider(provider)
            .map_err(|e| ProviderServiceError::InvalidInput(e.to_string()))?;

        // Secret lifecycle: omitted or empty never clears an existing secret.
        if let Some(secret) = input.api_key.as_deref() {
            if !secret.trim().is_empty() {
                self.storage
                    .save_provider_secret(&id, secret)
                    .map_err(|e| ProviderServiceError::InvalidInput(e.to_string()))?;
            }
        }

        self.reload()?;
        self.list()
    }

    pub fn delete(&self, id: &str) -> Result<Vec<ProviderData>, ProviderServiceError> {
        self.storage.delete_provider(id)?;
        self.reload()?;
        self.list()
    }

    pub fn save_credential(
        &self,
        id: &str,
        input: CredentialInput,
    ) -> Result<ProviderData, ProviderServiceError> {
        let secret = input
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ProviderServiceError::InvalidInput("api_key is required".into()))?;
        self.storage
            .save_provider_secret(id, secret)
            .map_err(|e| ProviderServiceError::InvalidInput(e.to_string()))?;

        let provider = self
            .storage
            .get_provider(id)?
            .ok_or_else(|| ProviderServiceError::NotFound(id.to_string()))?;
        let models = self.storage.load_models_for_provider(id)?;
        self.to_data(&provider, models)
    }

    pub fn delete_credential(&self, id: &str) -> Result<ProviderData, ProviderServiceError> {
        self.storage.delete_provider_secret(id)?;
        let provider = self
            .storage
            .get_provider(id)?
            .ok_or_else(|| ProviderServiceError::NotFound(id.to_string()))?;
        let models = self.storage.load_models_for_provider(id)?;
        self.to_data(&provider, models)
    }

    pub fn list_models(&self, provider_id: &str) -> Result<Vec<ModelData>, ProviderServiceError> {
        self.models_data(provider_id)
    }

    pub fn save_model(
        &self,
        provider_id: &str,
        input: ModelInput,
    ) -> Result<Vec<ModelData>, ProviderServiceError> {
        let model = ModelDefinition {
            id: input.id.unwrap_or_default(),
            provider_id: provider_id.to_string(),
            name: input.name.trim().to_string(),
            enabled: input.enabled.unwrap_or(true),
        };
        self.storage
            .upsert_model(model)
            .map_err(|e| ProviderServiceError::InvalidInput(e.to_string()))?;
        self.reload()?;
        self.models_data(provider_id)
    }

    pub fn delete_model(
        &self,
        provider_id: &str,
        model_id: &str,
    ) -> Result<Vec<ModelData>, ProviderServiceError> {
        self.storage.delete_model(model_id)?;
        self.reload()?;
        self.models_data(provider_id)
    }

    pub fn resolve(
        &self,
        query: &ResolveQuery,
    ) -> Result<ProviderResolveData, ProviderServiceError> {
        let registry = self.snapshot()?;
        let provider = registry
            .resolve_provider(query.provider.as_deref())
            .ok_or_else(|| ProviderServiceError::NotFound("provider".into()))?;
        let model = registry.resolve_model(provider, query.model.as_deref());

        Ok(ProviderResolveData {
            provider_id: provider.id.clone(),
            provider_name: provider.name.clone(),
            kind: provider.kind.clone(),
            is_default: provider.is_default,
            model_id: model.map(|m| m.id.clone()),
            model_name: model.map(|m| m.name.clone()),
        })
    }
}
