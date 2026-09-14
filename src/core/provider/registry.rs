// src/provider/registry.rs
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::provider::{ModelDefinition, ProviderDefinition};
use crate::storage::Storage;

/// In-memory view of the SQLite Provider/Model tables.
///
/// The database is the source of truth; [`ProviderRegistry::from_storage`]
/// rebuilds this view and [`reload_provider_registry`] swaps it into the shared
/// handle used by the running service.
///
/// The registry deliberately does not hold Provider secrets.
#[derive(Debug, Clone, Default)]
pub struct ProviderRegistry {
    providers: HashMap<String, ProviderDefinition>,
    aliases: HashMap<String, String>,
    models: HashMap<String, ModelDefinition>,
    default_id: Option<String>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_storage(storage: &Storage) -> anyhow::Result<Self> {
        let providers = storage.load_providers()?;
        let models = storage.load_models()?;
        Ok(Self::from_parts(providers, models))
    }

    pub fn from_parts(providers: Vec<ProviderDefinition>, models: Vec<ModelDefinition>) -> Self {
        let mut registry = Self::new();
        for provider in providers {
            if provider.is_default && provider.enabled {
                registry.default_id = Some(provider.id.clone());
            }
            registry
                .aliases
                .entry(provider.name.to_ascii_lowercase())
                .or_insert_with(|| provider.id.clone());
            registry.providers.insert(provider.id.clone(), provider);
        }
        for model in models {
            registry.models.insert(model.id.clone(), model);
        }
        registry
    }

    fn lookup(&self, id_or_name: &str) -> Option<&ProviderDefinition> {
        let key = id_or_name.trim();
        self.providers.get(key).or_else(|| {
            self.aliases
                .get(&key.to_ascii_lowercase())
                .and_then(|id| self.providers.get(id))
        })
    }

    pub fn get(&self, id_or_name: &str) -> Option<&ProviderDefinition> {
        self.lookup(id_or_name)
    }

    pub fn default_provider(&self) -> Option<&ProviderDefinition> {
        self.default_id
            .as_ref()
            .and_then(|id| self.providers.get(id))
            .filter(|p| p.enabled)
    }

    /// Resolve `None`/`"default"` to the default Provider, otherwise by id/name.
    pub fn resolve_provider(&self, requested: Option<&str>) -> Option<&ProviderDefinition> {
        match requested.map(str::trim).filter(|s| !s.is_empty()) {
            None | Some("default") => self.default_provider(),
            Some(key) => self.lookup(key).filter(|p| p.enabled),
        }
    }

    /// Resolve `None`/`"default"` to the first enabled model of `provider`,
    /// otherwise by model id or name.
    pub fn resolve_model(
        &self,
        provider: &ProviderDefinition,
        requested: Option<&str>,
    ) -> Option<&ModelDefinition> {
        let models = self.models_for(&provider.id);
        match requested.map(str::trim).filter(|s| !s.is_empty()) {
            None | Some("default") => models.into_iter().find(|m| m.enabled),
            Some(key) => models
                .into_iter()
                .find(|m| m.enabled && (m.id == key || m.name.eq_ignore_ascii_case(key))),
        }
    }

    pub fn models_for(&self, provider_id: &str) -> Vec<&ModelDefinition> {
        let mut models: Vec<&ModelDefinition> = self
            .models
            .values()
            .filter(|m| m.provider_id == provider_id)
            .collect();
        models.sort_by(|a, b| a.name.cmp(&b.name));
        models
    }

    pub fn providers(&self) -> Vec<&ProviderDefinition> {
        let mut list: Vec<&ProviderDefinition> = self.providers.values().collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    pub fn len(&self) -> usize {
        self.providers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}

pub type SharedProviderRegistry = Arc<RwLock<Arc<ProviderRegistry>>>;

pub fn shared_provider_registry(registry: ProviderRegistry) -> SharedProviderRegistry {
    Arc::new(RwLock::new(Arc::new(registry)))
}

/// Rebuild the registry from SQLite and atomically swap it into `shared`.
pub fn reload_provider_registry(
    shared: &SharedProviderRegistry,
    storage: &Storage,
) -> anyhow::Result<()> {
    let registry = ProviderRegistry::from_storage(storage)?;
    if let Ok(mut guard) = shared.write() {
        *guard = Arc::new(registry);
    }
    Ok(())
}
