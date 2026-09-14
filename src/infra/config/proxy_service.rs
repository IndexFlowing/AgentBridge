// src/infra/config/proxy_service.rs
//! Application Service for Network Proxy persistence, patch merge, and testing.

use std::sync::Arc;
use thiserror::Error;

use crate::config::{apply_proxy_patch, ProxyConfig, ProxyPatch};
use crate::models::{ProxyData, SaveProxyRequest, TestProxyRequest};
use crate::projects::ProjectHub;
use crate::storage::proxies::ProxyDefinition;
use crate::storage::Storage;

#[derive(Debug, Error)]
pub enum ProxyServiceError {
    #[error(transparent)]
    Executor(#[from] crate::executor::ExecutorError),
    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}

#[derive(Clone)]
pub struct ProxyService {
    hub: Arc<ProjectHub>,
    storage: Arc<Storage>,
}

impl ProxyService {
    pub fn new(hub: Arc<ProjectHub>, storage: Arc<Storage>) -> Self {
        Self { hub, storage }
    }

    pub fn get(&self) -> Result<ProxyData, ProxyServiceError> {
        let proxies = self.storage.load_proxies()?;
        let active = proxies
            .into_iter()
            .find(|p| p.is_default)
            .or_else(|| self.storage.load_proxies().ok()?.into_iter().next());

        match active {
            Some(p) => Ok(ProxyData::from(&p.to_config())),
            None => Ok(ProxyData::from(&ProxyConfig::default())),
        }
    }

    pub fn save(&self, req: SaveProxyRequest) -> Result<ProxyData, ProxyServiceError> {
        let proxies = self.storage.load_proxies()?;
        let current_def = proxies
            .into_iter()
            .find(|p| p.id == "default" || p.is_default);

        let current_cfg = current_def
            .as_ref()
            .map(|d| d.to_config())
            .unwrap_or_default();

        let patch = ProxyPatch {
            enabled: Some(req.enabled),
            kind: Some(req.kind),
            host: Some(req.host),
            port: Some(req.port),
            username: req.username,
            password: req.password,
        };
        let updated_cfg = apply_proxy_patch(&current_cfg, patch)?;

        let def = ProxyDefinition {
            id: current_def
                .as_ref()
                .map(|d| d.id.clone())
                .unwrap_or_else(|| "default".to_string()),
            name: current_def
                .as_ref()
                .map(|d| d.name.clone())
                .unwrap_or_else(|| "Default Proxy".to_string()),
            kind: updated_cfg.kind,
            host: updated_cfg.host,
            port: updated_cfg.port,
            username: updated_cfg.username,
            password: updated_cfg.password,
            enabled: updated_cfg.enabled,
            is_default: true,
        };

        self.storage.upsert_proxy(def)?;
        self.hub.reload_executors()?;
        self.get()
    }

    pub async fn test(&self, req: TestProxyRequest) -> Result<String, ProxyServiceError> {
        let cfg = ProxyConfig {
            enabled: true,
            kind: req.kind,
            host: req.host.trim().to_string(),
            port: req.port,
            username: req.username.filter(|u| !u.trim().is_empty()),
            password: req.password.filter(|p| !p.trim().is_empty()),
        };
        crate::executor::proxy::test_proxy(&cfg)
            .await
            .map_err(ProxyServiceError::Executor)
    }
}