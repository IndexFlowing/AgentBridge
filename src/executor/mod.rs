pub mod discovery;
pub mod opencode;
pub mod output;
pub mod process;
pub mod proxy;

use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tokio::process::Child;

pub use discovery::{
    common_executor_definitions, executor_definitions_with_discovery, opencode_version,
    scan_executor,
};
pub use opencode::{validate_executor_type, OpenCodeExecutor};
pub use output::{extract_tests_excerpt, run_spawned, strip_reasoning};
pub use process::{find_executable, kill_process_tree, process_is_alive};
pub use proxy::test_proxy;

use crate::config::{Config, ExecutorDefinition, ProxyConfig};
use crate::protocol::C2cPlan;

#[derive(Debug, thiserror::Error)]
pub enum ExecutorError {
    #[error(
        "OpenCode is not installed or not found on PATH (looked for `{0}`). \
         Install OpenCode and ensure the executable is available."
    )]
    NotInstalled(String),
    #[error("executor type `{0}` is not allowed (allowlist: opencode, codex, claude)")]
    TypeNotAllowed(String),
    #[error("executor type `{0}` is not implemented; V0.2 only supports OpenCode")]
    TypeNotImplemented(String),
    #[error("executor `{0}` was not found in the registry")]
    NotFound(String),
    #[error("an executor task is already running (task_id={0})")]
    AlreadyRunning(String),
    #[error("no running executor task")]
    NotRunning,
    #[error("executor command is invalid: {0}")]
    InvalidCommand(String),
    #[error("executor workspace is invalid: {0}")]
    InvalidWorkspace(String),
    #[error("failed to start OpenCode: {0}")]
    Spawn(String),
    #[error("executor crashed: {0}")]
    Crashed(String),
    #[error("executor was cancelled")]
    Cancelled,
    #[error("{0}")]
    Other(String),
}

/// 可插拔执行器抽象
pub trait Executor: Send + Sync {
    fn id(&self) -> &str;
    fn kind(&self) -> &str;
    fn name(&self) -> &str;
    fn detect(&self) -> Result<PathBuf, ExecutorError>;
    fn start_task(
        &self,
        plan: &C2cPlan,
        workspace: &Path,
        proxy: Option<&ProxyConfig>,
    ) -> Result<SpawnedTask, ExecutorError>;
}

/// 统一执行器注册与 Proxy 路由分发器
#[derive(Clone)]
pub struct ExecutorRegistry {
    executors: HashMap<String, Arc<dyn Executor>>,
    definitions: HashMap<String, ExecutorDefinition>,
    global_proxy: ProxyConfig,
}

impl ExecutorRegistry {
    pub fn new(global_proxy: ProxyConfig) -> Self {
        Self {
            executors: HashMap::new(),
            definitions: HashMap::new(),
            global_proxy,
        }
    }

    pub fn from_config(
        config: &Config,
        definitions: &[ExecutorDefinition],
    ) -> Result<Self, ExecutorError> {
        let mut registry = Self::new(config.proxy.clone());

        // 1. 注册默认全局 OpenCode 执行器
        let default_opencode = OpenCodeExecutor::from_config(&config.executor)?;
        registry.register("opencode", Arc::new(default_opencode), None);

        // 2. 加载定义的全部执行器
        for def in definitions {
            if !def.enabled {
                continue;
            }
            if def.kind == "opencode" {
                let cmd = if def.command.trim().is_empty() {
                    def.executable
                        .as_deref()
                        .and_then(|p| p.to_str())
                        .unwrap_or("opencode")
                } else {
                    &def.command
                };
                let exec: Arc<dyn Executor> = Arc::new(OpenCodeExecutor::new(
                    &def.id,
                    &def.name,
                    cmd,
                    config.executor.mode,
                )?);
                registry.register(&def.id, exec.clone(), Some(def.clone()));
                let lower_name = def.name.to_ascii_lowercase();
                if !registry.executors.contains_key(&lower_name) {
                    registry.executors.insert(lower_name, exec);
                }
            }
        }

        Ok(registry)
    }

    pub fn register(
        &mut self,
        key: &str,
        executor: Arc<dyn Executor>,
        def: Option<ExecutorDefinition>,
    ) {
        self.executors.insert(key.to_string(), executor);
        if let Some(def) = def {
            self.definitions.insert(key.to_string(), def);
        }
    }

    pub fn get(&self, id_or_name: &str) -> Option<Arc<dyn Executor>> {
        self.executors
            .get(id_or_name)
            .or_else(|| self.executors.get(&id_or_name.to_ascii_lowercase()))
            .cloned()
    }

    /// 依据 Executor 定义的 proxy_id 检索匹配的 Proxy；若未显式指定，则 fallback 到全局默认 Proxy
    pub fn resolve_proxy_for(&self, id_or_name: &str) -> Option<ProxyConfig> {
        if let Some(def) = self.definitions.get(id_or_name) {
            if let Some(proxy_id) = &def.proxy_id {
                if proxy_id == "default" {
                    return if self.global_proxy.enabled {
                        Some(self.global_proxy.clone())
                    } else {
                        None
                    };
                }
            }
        }
        if self.global_proxy.enabled {
            Some(self.global_proxy.clone())
        } else {
            None
        }
    }
}

/// Shared, swappable handle to the currently active [`ExecutorRegistry`].
///
/// Long-lived `ProjectHub`/`TaskRuntime` values hold this handle rather than a
/// concrete registry, so reloading the registry from storage becomes visible to
/// subsequent task executions without restarting the process.
pub type SharedExecutorRegistry = Arc<RwLock<Arc<ExecutorRegistry>>>;

/// Wrap a freshly built registry in the shared handle used by the service.
pub fn shared_registry(registry: ExecutorRegistry) -> SharedExecutorRegistry {
    Arc::new(RwLock::new(Arc::new(registry)))
}

pub struct SpawnedTask {
    pub child: Child,
    pub pid: u32,
    pub executable: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutorSnapshot {
    pub task_id: String,
    pub status: String,
    pub summary: Option<String>,
    pub exit_code: Option<i32>,
    pub tests: Option<String>,
    pub changed_files: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutorOutcome {
    pub exit_code: Option<i32>,
    pub summary: String,
    pub tests_excerpt: Option<String>,
    pub cancelled: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutorAvailability {
    pub id: String,
    pub available: bool,
    pub executable: Option<PathBuf>,
    pub version: Option<String>,
    pub error: Option<String>,
    pub status: ExecutorAvailabilityStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutorAvailabilityStatus {
    Available,
    NotFound,
    NotExecutable,
    VersionProbeFailed,
}

impl ExecutorAvailabilityStatus {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::NotFound => "not_found",
            Self::NotExecutable => "not_executable",
            Self::VersionProbeFailed => "version_probe_failed",
        }
    }
}

impl std::fmt::Display for ExecutorAvailabilityStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct ExecutorView {
    pub definition: ExecutorDefinition,
    pub detected: bool,
    pub availability: ExecutorAvailability,
}

pub fn list_views(config_path: &Path) -> anyhow::Result<Vec<ExecutorView>> {
    let registry = crate::config::load_executor_registry(config_path)?;
    Ok(executor_definitions_with_discovery(&registry.executors)
        .into_iter()
        .map(|(definition, detected)| {
            let availability = scan_executor(&definition);
            ExecutorView {
                definition,
                detected,
                availability,
            }
        })
        .collect())
}

pub fn available_kinds(config_path: &Path) -> anyhow::Result<Vec<String>> {
    let mut names = Vec::new();
    for view in list_views(config_path)? {
        if view.availability.available || view.definition.enabled {
            if !names.contains(&view.definition.kind) {
                names.push(view.definition.kind);
            }
        }
    }
    if names.is_empty() {
        names.push("opencode".into());
    }
    Ok(names)
}
