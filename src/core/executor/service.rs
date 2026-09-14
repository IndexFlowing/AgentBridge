// src/core/executor/service.rs
//! Application Service for Local AI coding executor registration and probing.

use std::sync::Arc;
use thiserror::Error;

use crate::config::ExecutorDefinition;
use crate::executor::{common_executor_definitions, scan_executor, ExecutorView};
use crate::models::{ExecutorData, SaveExecutorRequest};
use crate::projects::ProjectHub;
use crate::storage::Storage;

#[derive(Debug, Error)]
pub enum ExecutorServiceError {
    #[error("Executor '{0}' not found")]
    NotFound(String),
    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}

#[derive(Clone)]
pub struct ExecutorService {
    hub: Arc<ProjectHub>,
    storage: Arc<Storage>,
}

impl ExecutorService {
    pub fn new(hub: Arc<ProjectHub>, storage: Arc<Storage>) -> Self {
        Self { hub, storage }
    }

    pub fn available(&self) -> Vec<String> {
        vec!["opencode".to_string()]
    }

    pub fn list(&self) -> Result<Vec<ExecutorData>, ExecutorServiceError> {
        let custom_defs = self.storage.load_executors()?;
        let mut all_defs = common_executor_definitions();
        all_defs.extend(custom_defs);

        let mut views = Vec::new();
        for def in all_defs {
            let availability = scan_executor(&def);
            views.push(ExecutorData::from(ExecutorView {
                definition: def,
                detected: false,
                availability,
            }));
        }
        Ok(views)
    }

    pub fn save(
        &self,
        req: SaveExecutorRequest,
    ) -> Result<Vec<ExecutorData>, ExecutorServiceError> {
        let def = ExecutorDefinition {
            id: req.id.unwrap_or_default(),
            display_name: req.name.clone(),
            name: req.name,
            kind: req.kind,
            command: req.command,
            executable: req.executable.filter(|v| !v.as_os_str().is_empty()),
            working_directory: req.working_directory.filter(|v| !v.as_os_str().is_empty()),
            proxy_id: req.proxy_id.filter(|v| !v.trim().is_empty()),
            enabled: req.enabled,
        };

        self.storage.upsert_executor(def)?;
        self.hub.reload_executors()?;
        self.list()
    }

    pub fn delete(&self, id: &str) -> Result<Vec<ExecutorData>, ExecutorServiceError> {
        self.storage.delete_executor(id)?;
        self.hub.reload_executors()?;
        self.list()
    }

    pub fn test(&self, id: &str) -> Result<ExecutorData, ExecutorServiceError> {
        let list = self.list()?;
        list.into_iter()
            .find(|e| e.id == id)
            .ok_or_else(|| ExecutorServiceError::NotFound(id.to_string()))
    }
}
