// src/provider.rs
//! Provider Core: data model, storage-backed runtime registry, resolution, and encrypted credentials.
//!
//! Providers are deliberately independent from Executors and Proxies.

pub mod credentials;
pub mod registry;
pub mod service;
pub mod types;

pub use credentials::{CredentialCipher, CREDENTIAL_KEY_ENV, CREDENTIAL_SCHEME};
pub use registry::{
    reload_provider_registry, shared_provider_registry, ProviderRegistry, SharedProviderRegistry,
};
pub use service::{ProviderService, ProviderServiceError};
pub use types::{CredentialMetadata, ModelDefinition, ProviderDefinition};
