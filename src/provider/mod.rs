// src/provider/mod.rs
//!
//! Provider Core: data model, storage-backed runtime registry, and resolution.
//!
//! Providers are deliberately independent from Executors and Proxies. A
//! Provider may carry an optional `proxy_id` reference, but the Proxy layer
//! never depends on Providers.

pub mod registry;
pub mod types;

pub use registry::{
    reload_provider_registry, shared_provider_registry, ProviderRegistry, SharedProviderRegistry,
};
pub use types::{CredentialMetadata, ModelDefinition, ProviderDefinition};
