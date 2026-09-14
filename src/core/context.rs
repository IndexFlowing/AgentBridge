// src/core/context.rs
//! Unified Application Core Context (Dependency Injection Container).

use std::sync::Arc;

use crate::config::Config;
use crate::config::ProxyService;
use crate::executor::ExecutorService;
use crate::projects::{ProjectHub, ProjectService};
use crate::provider::{shared_provider_registry, ProviderRegistry, SharedProviderRegistry};
use crate::storage::Storage;
use crate::task::TaskService;

/// The central application core container.
///
/// Holds singletons for all domain services, shared registries, and storage.
#[derive(Clone)]
pub struct AppCore {
    pub config: Arc<Config>,
    pub storage: Arc<Storage>,
    pub hub: Arc<ProjectHub>,
    pub providers: SharedProviderRegistry,
    pub tasks: Arc<TaskService>,
    pub projects: Arc<ProjectService>,
    pub executors: Arc<ExecutorService>,
    pub proxies: Arc<ProxyService>,
}

impl AppCore {
    /// Assemble and initialize the entire core system.
    pub fn new(config: Arc<Config>, storage: Arc<Storage>, hub: Arc<ProjectHub>) -> Self {
        let providers = shared_provider_registry(
            ProviderRegistry::from_storage(&storage).unwrap_or_default(),
        );
        let tasks = Arc::new(TaskService::new(hub.clone(), storage.clone()));
        let projects = Arc::new(ProjectService::new(hub.clone(), storage.clone()));
        let executors = Arc::new(ExecutorService::new(hub.clone(), storage.clone()));
        let proxies = Arc::new(ProxyService::new(hub.clone(), storage.clone()));

        Self {
            config,
            storage,
            hub,
            providers,
            tasks,
            projects,
            executors,
            proxies,
        }
    }
}