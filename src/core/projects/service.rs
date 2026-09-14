// src/core/projects/service.rs
//! Application Service for Project workspace management and isolation.

use std::sync::Arc;
use thiserror::Error;

use crate::models::SaveProjectRequest;
use crate::projects::{ProjectEntry, ProjectHub, ProjectListing};
use crate::storage::Storage;
use crate::workspace::Workspace;

#[derive(Debug, Error)]
pub enum ProjectServiceError {
    #[error("Executor is required")]
    ExecutorRequired,
    #[error("Project '{0}' has an active running task; modification is rejected")]
    TaskRunning(String),
    #[error("Project '{0}' not found")]
    NotFound(String),
    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}

#[derive(Clone)]
pub struct ProjectService {
    hub: Arc<ProjectHub>,
    storage: Arc<Storage>,
}

impl ProjectService {
    pub fn new(hub: Arc<ProjectHub>, storage: Arc<Storage>) -> Self {
        Self { hub, storage }
    }

    pub fn list(&self) -> Result<Vec<ProjectListing>, ProjectServiceError> {
        let projects = self.storage.load_projects()?;
        let active = self.hub.default_name();
        let list = projects.iter().map(|p| to_listing(p, &active)).collect();
        Ok(list)
    }

    pub async fn save(&self, req: SaveProjectRequest) -> Result<Vec<ProjectListing>, ProjectServiceError> {
        let executor = req.executor.trim().to_ascii_lowercase();
        if executor.is_empty() {
            return Err(ProjectServiceError::ExecutorRequired);
        }

        // 活跃任务安全拦截
        if let Some(existing) = self.hub.get(&req.name) {
            if existing.runtime.is_running().await {
                return Err(ProjectServiceError::TaskRunning(req.name.clone()));
            }
        }
        if let Some(ref id) = req.id {
            if let Ok(projects) = self.storage.load_projects() {
                if let Some(old_p) = projects.iter().find(|p| &p.id == id) {
                    if old_p.name != req.name {
                        if let Some(existing) = self.hub.get(&old_p.name) {
                            if existing.runtime.is_running().await {
                                return Err(ProjectServiceError::TaskRunning(old_p.name.clone()));
                            }
                        }
                    }
                }
            }
        }

        let entry = ProjectEntry {
            id: req.id.unwrap_or_default(),
            name: req.name,
            path: req.path,
            description: req.description,
            readonly: req.readonly,
            executor,
        };

        self.storage.upsert_project(entry)?;
        let _ = self.hub.reload();
        self.list()
    }

    pub async fn delete(&self, id: &str) -> Result<Vec<ProjectListing>, ProjectServiceError> {
        if let Ok(projects) = self.storage.load_projects() {
            if let Some(target) = projects.iter().find(|p| p.id == id) {
                if let Some(existing) = self.hub.get(&target.name) {
                    if existing.runtime.is_running().await {
                        return Err(ProjectServiceError::TaskRunning(target.name.clone()));
                    }
                }
            }
        }

        self.storage.delete_project(id)?;
        let _ = self.hub.reload();
        self.list()
    }
}

fn to_listing(entry: &ProjectEntry, active_name: &str) -> ProjectListing {
    let (git_repository, project_type) = if entry.path.is_dir() {
        if let Ok(ws) = Workspace::open(&entry.path, 1_048_576, false) {
            let info = ws.info();
            (info.git_repository, info.project_type)
        } else {
            (false, vec![])
        }
    } else {
        (false, vec![])
    };

    ProjectListing {
        id: entry.id.clone(),
        name: entry.name.clone(),
        path: entry.path.to_string_lossy().to_string(),
        description: entry.description.clone(),
        readonly: entry.readonly,
        active: entry.name == active_name,
        git_repository,
        project_type,
        executor: entry.executor.clone(),
    }
}