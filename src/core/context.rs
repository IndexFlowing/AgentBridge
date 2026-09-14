// src/core/context.rs
//! Unified Application Core Context (Dependency Injection Container).

use std::sync::Arc;

use crate::config::Config;
use crate::config::ProxyService;
use crate::executor::ExecutorService;
use crate::projects::{ProjectHub, ProjectService};
use crate::provider::{
    shared_provider_registry, ProviderRegistry, ProviderService, SharedProviderRegistry,
};
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
    pub provider_service: Arc<ProviderService>,
    pub tasks: Arc<TaskService>,
    pub projects: Arc<ProjectService>,
    pub executors: Arc<ExecutorService>,
    pub proxies: Arc<ProxyService>,
    pub skills: Arc<crate::core::skill::SkillService>,
}

impl AppCore {
    /// Assemble and initialize the entire core system.
    pub fn new(config: Arc<Config>, storage: Arc<Storage>, hub: Arc<ProjectHub>) -> Self {
        let providers =
            shared_provider_registry(ProviderRegistry::from_storage(&storage).unwrap_or_default());
        let provider_service = Arc::new(ProviderService::new(storage.clone(), providers.clone()));
        let tasks = Arc::new(TaskService::new(hub.clone(), storage.clone()));
        let projects = Arc::new(ProjectService::new(hub.clone(), storage.clone()));
        let executors = Arc::new(ExecutorService::new(hub.clone(), storage.clone()));
        let proxies = Arc::new(ProxyService::new(hub.clone(), storage.clone()));

        let home = dirs::home_dir().expect("Cannot locate home directory");
        let skills_dir = home.join(".agentbridge").join("skills");
        let skills = Arc::new(
            crate::core::skill::SkillService::new(storage.clone(), skills_dir)
                .with_projects(hub.clone()),
        );
        Self {
            config,
            storage,
            hub,
            providers,
            provider_service,
            tasks,
            projects,
            executors,
            proxies,
            skills,
        }
    }

    /// Composition root for process entry points.
    ///
    /// Opens the shared SQLite storage, builds the `ProjectHub`, and assembles
    /// every domain service. CLI and server entry points must go through this
    /// (or [`AppCore::new`]) rather than repeating the wiring themselves.
    pub fn bootstrap(config: Arc<Config>) -> anyhow::Result<Self> {
        let storage = Arc::new(Storage::init()?);
        let hub = Arc::new(ProjectHub::new(config.clone(), storage.clone())?);
        Ok(Self::new(config, storage, hub))
    }
}
