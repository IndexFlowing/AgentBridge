// src/provider/types.rs
use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

/// A configurable AI Provider (OpenCode, Claude, Gemini, DeepSeek, ...).
///
/// Credentials are intentionally *not* part of this struct; they live in a
/// separate credential store keyed by `id`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderDefinition {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub is_default: bool,
    /// Optional reference to a Proxy. Proxy never references a Provider.
    #[serde(default)]
    pub proxy_id: Option<String>,
}

impl ProviderDefinition {
    pub fn new(name: impl Into<String>, kind: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            kind: kind.into().trim().to_ascii_lowercase(),
            base_url: String::new(),
            enabled: true,
            is_default: false,
            proxy_id: None,
        }
    }
}

/// A model exposed by a Provider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelDefinition {
    pub id: String,
    pub provider_id: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl ModelDefinition {
    pub fn new(provider_id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            provider_id: provider_id.into(),
            name: name.into(),
            enabled: true,
        }
    }
}

/// Non-secret credential metadata safe to expose through API responses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialMetadata {
    pub provider_id: String,
    pub configured: bool,
    pub updated_at: Option<String>,
}

impl CredentialMetadata {
    pub fn missing(provider_id: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            configured: false,
            updated_at: None,
        }
    }
}
