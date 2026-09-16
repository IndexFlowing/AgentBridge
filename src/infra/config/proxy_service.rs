// src/infra/config/proxy_service.rs
//! Application Service for Network Proxy persistence, patch merge, and testing.

use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

use crate::config::{apply_proxy_patch, ProxyConfig, ProxyPatch};
use crate::executor::proxy::ProxyTestReport;
use crate::models::{
    ProxyData, ProxyVerifyResult, SaveProxyRequest, TestProxyRequest,
};
use crate::projects::ProjectHub;
use crate::storage::proxies::ProxyDefinition;
use crate::storage::Storage;

#[derive(Debug, Error)]
pub enum ProxyServiceError {
    #[error("Proxy '{0}' not found")]
    NotFound(String),
    #[error("代理 '{id}' 仍被以下执行器引用，无法删除：{executors}")]
    InUse { id: String, executors: String },
    #[error("{0}")]
    Invalid(String),
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

    fn all(&self) -> Result<Vec<ProxyDefinition>, ProxyServiceError> {
        let mut proxies = self.storage.load_proxies()?;
        proxies.sort_by(|a, b| {
            b.is_default
                .cmp(&a.is_default)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(proxies)
    }

    /// The active default proxy, or an empty placeholder when none is stored.
    pub fn get(&self) -> Result<ProxyData, ProxyServiceError> {
        let proxies = self.all()?;
        match proxies.first() {
            Some(p) => Ok(ProxyData::from(p)),
            None => Ok(ProxyData::from(&ProxyConfig::default())),
        }
    }

    pub fn list(&self) -> Result<Vec<ProxyData>, ProxyServiceError> {
        Ok(self.all()?.iter().map(ProxyData::from).collect())
    }

    pub fn get_by_id(&self, id: &str) -> Result<ProxyData, ProxyServiceError> {
        self.required(id).map(|def| ProxyData::from(&def))
    }

    fn required(&self, id: &str) -> Result<ProxyDefinition, ProxyServiceError> {
        self.storage
            .load_proxy(id)?
            .ok_or_else(|| ProxyServiceError::NotFound(id.to_string()))
    }

    /// Legacy single-default save used by `PUT /proxy`.
    pub fn save(&self, req: SaveProxyRequest) -> Result<ProxyData, ProxyServiceError> {
        let proxies = self.all()?;
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
        let updated_cfg = apply_proxy_patch(&current_cfg, patch)
            .map_err(|e| ProxyServiceError::Invalid(e.to_string()))?;

        let def = ProxyDefinition {
            id: current_def
                .as_ref()
                .map(|d| d.id.clone())
                .unwrap_or_else(|| "default".to_string()),
            name: current_def
                .as_ref()
                .map(|d| d.name.clone())
                .unwrap_or_else(|| "默认代理".to_string()),
            kind: updated_cfg.kind,
            host: updated_cfg.host,
            port: updated_cfg.port,
            username: updated_cfg.username,
            password: updated_cfg.password,
            enabled: updated_cfg.enabled,
            is_default: true,
            test_url: req
                .test_url
                .filter(|u| !u.trim().is_empty())
                .or_else(|| current_def.as_ref().map(|d| d.test_url.clone()))
                .unwrap_or_default()
                .trim()
                .to_string(),
            last_verified_at: current_def.as_ref().and_then(|d| d.last_verified_at.clone()),
            last_verified_ok: current_def.as_ref().and_then(|d| d.last_verified_ok),
            last_verified_latency_ms: current_def
                .as_ref()
                .and_then(|d| d.last_verified_latency_ms),
        };

        self.storage.upsert_proxy(def)?;
        self.hub.reload_executors()?;
        self.get()
    }

    /// Create a new proxy. The first proxy created becomes the default.
    pub fn create(&self, req: SaveProxyRequest) -> Result<Vec<ProxyData>, ProxyServiceError> {
        let cfg = ProxyConfig {
            enabled: req.enabled,
            kind: req.kind,
            host: req.host.trim().to_string(),
            port: req.port,
            username: normalize_secret(req.username),
            password: normalize_secret(req.password),
        };
        cfg.validate()
            .map_err(|e| ProxyServiceError::Invalid(e.to_string()))?;

        let existing = self.all()?;
        let is_default = req.is_default || existing.is_empty();
        let id = req
            .id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let def = ProxyDefinition {
            id,
            name: normalize_name(&req.name),
            kind: cfg.kind,
            host: cfg.host,
            port: cfg.port,
            username: cfg.username,
            password: cfg.password,
            enabled: cfg.enabled,
            is_default,
            test_url: normalize_test_url(req.test_url),
            last_verified_at: None,
            last_verified_ok: None,
            last_verified_latency_ms: None,
        };
        self.storage.upsert_proxy(def)?;
        self.hub.reload_executors()?;
        self.list()
    }

    /// Update an existing proxy while preserving its secret when omitted.
    pub fn update(
        &self,
        id: &str,
        req: SaveProxyRequest,
    ) -> Result<Vec<ProxyData>, ProxyServiceError> {
        let current = self.required(id)?;
        let current_cfg = current.to_config();
        let patch = ProxyPatch {
            enabled: Some(req.enabled),
            kind: Some(req.kind),
            host: Some(req.host),
            port: Some(req.port),
            username: req.username,
            password: req.password,
        };
        let cfg = apply_proxy_patch(&current_cfg, patch)
            .map_err(|e| ProxyServiceError::Invalid(e.to_string()))?;

        let test_url = match req.test_url {
            Some(url) if !url.trim().is_empty() => url.trim().to_string(),
            _ => current.test_url.clone(),
        };

        let def = ProxyDefinition {
            id: current.id,
            name: normalize_name(&req.name),
            kind: cfg.kind,
            host: cfg.host,
            port: cfg.port,
            username: cfg.username,
            password: cfg.password,
            enabled: cfg.enabled,
            is_default: req.is_default || current.is_default,
            test_url,
            last_verified_at: current.last_verified_at,
            last_verified_ok: current.last_verified_ok,
            last_verified_latency_ms: current.last_verified_latency_ms,
        };
        self.storage.upsert_proxy(def)?;
        self.hub.reload_executors()?;
        self.list()
    }

    pub fn set_enabled(
        &self,
        id: &str,
        enabled: bool,
    ) -> Result<Vec<ProxyData>, ProxyServiceError> {
        let current = self.required(id)?;
        let def = ProxyDefinition { enabled, ..current };
        self.storage.upsert_proxy(def)?;
        self.hub.reload_executors()?;
        self.list()
    }

    pub fn delete(&self, id: &str) -> Result<Vec<ProxyData>, ProxyServiceError> {
        let _ = self.required(id)?;
        let referencing = self.storage.load_executors()?;
        let in_use: Vec<String> = referencing
            .iter()
            .filter(|e| e.proxy_id.as_deref() == Some(id))
            .map(|e| e.name.clone())
            .collect();
        if !in_use.is_empty() {
            return Err(ProxyServiceError::InUse {
                id: id.to_string(),
                executors: in_use.join("、"),
            });
        }
        self.storage.delete_proxy(id)?;
        self.hub.reload_executors()?;
        self.list()
    }

    /// Verify connectivity for a proxy that is already persisted, recording the
    /// outcome so the UI can show the last status and latency.
    pub async fn verify_id(&self, id: &str) -> Result<ProxyVerifyResult, ProxyServiceError> {
        let def = self.required(id)?;
        let cfg = def.to_config();
        let report =
            crate::executor::proxy::test_proxy(&cfg, &def.test_url, test_timeout()).await?;
        self.storage
            .record_proxy_verification(id, report.success, report.latency_ms)?;
        Ok(self.to_verify_result(report, Some(id)))
    }

    /// Verify an unsaved/draft proxy configuration without persisting it.
    pub async fn verify(&self, req: TestProxyRequest) -> Result<ProxyVerifyResult, ProxyServiceError> {
        let cfg = ProxyConfig {
            enabled: true,
            kind: req.kind,
            host: req.host.trim().to_string(),
            port: req.port,
            username: normalize_secret(req.username),
            password: normalize_secret(req.password),
        };
        let target = req
            .test_url
            .map(|u| u.trim().to_string())
            .filter(|u| !u.is_empty())
            .unwrap_or_default();
        let report = crate::executor::proxy::test_proxy(&cfg, &target, test_timeout()).await?;
        Ok(self.to_verify_result(report, None))
    }

    fn to_verify_result(
        &self,
        report: ProxyTestReport,
        persisted_id: Option<&str>,
    ) -> ProxyVerifyResult {
        let verified_at = persisted_id
            .and_then(|id| self.storage.load_proxy(id).ok().flatten())
            .and_then(|p| p.last_verified_at);
        ProxyVerifyResult {
            success: report.success,
            latency_ms: report.latency_ms,
            message: report.message,
            target: report.target,
            verified_at,
        }
    }
}

fn test_timeout() -> Duration {
    crate::executor::proxy::DEFAULT_TEST_TIMEOUT
}

fn normalize_secret(value: Option<String>) -> Option<String> {
    value.filter(|v| !v.trim().is_empty())
}

fn normalize_name(raw: &str) -> String {
    let name = raw.trim();
    if name.is_empty() {
        "未命名代理".to_string()
    } else {
        name.to_string()
    }
}

fn normalize_test_url(raw: Option<String>) -> String {
    raw.map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty())
        .unwrap_or_default()
}
