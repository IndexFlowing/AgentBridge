// src/executor/mod.rs
pub mod antigravity;
pub mod discovery;
pub mod handoff;
pub mod opencode;
pub mod output;
pub mod process;
pub mod proxy;
pub mod service;

use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tokio::process::Child;

pub use antigravity::AntigravityExecutor;
pub use discovery::{
    common_executor_definitions, executor_definitions_with_discovery, opencode_version,
    scan_executor,
};
pub use opencode::{validate_executor_type, OpenCodeExecutor};
pub use output::{extract_tests_excerpt, run_spawned, strip_reasoning};
pub use process::{
    find_executable, kill_process_tree, parse_command, process_is_alive, spawn_cli,
};
pub use proxy::test_proxy;
pub use service::{ExecutorService, ExecutorServiceError};

use crate::config::{Config, ExecutorDefinition, ProxyConfig};
use crate::core::agent::AgentContext;
use crate::protocol::C2cPlan;
use crate::storage::proxies::ProxyDefinition;

#[derive(Debug, thiserror::Error)]
pub enum ExecutorError {
    #[error(
        "executor `{0}` is not installed or not found on PATH. \
         Install it and ensure the executable is available."
    )]
    NotInstalled(String),
    #[error("executor type `{0}` is not allowed (allowlist: opencode, antigravity, codex, claude)")]
    TypeNotAllowed(String),
    #[error("executor type `{0}` is not implemented; AgentBridge supports opencode and antigravity")]
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
        context: &AgentContext,
        workspace: &Path,
        proxy: Option<&ProxyConfig>,
    ) -> Result<SpawnedTask, ExecutorError>;
}

/// 统一执行器注册与 Proxy 路由分发器
#[derive(Clone)]
pub struct ExecutorRegistry {
    executors: HashMap<String, Arc<dyn Executor>>,
    definitions: HashMap<String, ExecutorDefinition>,
    proxies: HashMap<String, ProxyConfig>,
    default_proxy: Option<ProxyConfig>,
}

impl Default for ExecutorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutorRegistry {
    pub fn new() -> Self {
        Self {
            executors: HashMap::new(),
            definitions: HashMap::new(),
            proxies: HashMap::new(),
            default_proxy: None,
        }
    }

    pub fn from_config(
        config: &Config,
        definitions: &[ExecutorDefinition],
        proxies: &[ProxyDefinition],
    ) -> Result<Self, ExecutorError> {
        let mut registry = Self::new();

        // 1. 载入所有已配置的代理
        for p in proxies {
            let cfg = p.to_config();
            if p.is_default && p.enabled {
                registry.default_proxy = Some(cfg.clone());
            }
            registry.proxies.insert(p.id.clone(), cfg);
        }
        // 如果 SQLite 没有默认代理，尝试使用 config.toml 的全局代理作为兜底
        if registry.default_proxy.is_none() && config.proxy.enabled {
            registry.default_proxy = Some(config.proxy.clone());
        }

        // 2. 加载 SQLite 中配置的执行器（SQLite 为最终真值）
        for def in definitions {
            if !def.enabled {
                continue;
            }
            // 按 kind 选择实现：新增执行器只需在这里多一个分支，代理与进程
            // 能力由共享抽象提供，执行器自身不承载代理逻辑。
            let command = executor_command(def);
            let exec: Arc<dyn Executor> = match def.kind.as_str() {
                "opencode" => Arc::new(OpenCodeExecutor::new(
                    &def.id,
                    &def.name,
                    command,
                    config.executor.mode,
                )?),
                "antigravity" => Arc::new(AntigravityExecutor::new(
                    &def.id,
                    &def.name,
                    command,
                    config.executor.mode,
                )?),
                _ => continue,
            };
            registry.register(&def.id, exec.clone(), Some(def.clone()));

            let lower_name = def.name.to_ascii_lowercase();
            registry.register_alias(&lower_name, exec.clone(), &def);

            // 【核心真值优先级】：builtin 或与 kind 同名的执行器接管 kind 键
            if def.id == format!("builtin-{}", def.kind) || def.name.eq_ignore_ascii_case(&def.kind)
            {
                registry.executors.insert(def.kind.clone(), exec.clone());
                registry.definitions.insert(def.kind.clone(), def.clone());
            }
        }

        // 3. 仅当 SQLite 中完全未配置对应 kind 时，才由 config.toml 的兜底配置占位
        match config.executor.kind.trim().to_ascii_lowercase().as_str() {
            "antigravity" => {
                if !registry.executors.contains_key("antigravity") {
                    registry.register(
                        "antigravity",
                        Arc::new(AntigravityExecutor::from_config(&config.executor)?),
                        None,
                    );
                }
            }
            _ => {
                if !registry.executors.contains_key("opencode") {
                    let default_opencode = OpenCodeExecutor::from_config(&config.executor)?;
                    registry.register("opencode", Arc::new(default_opencode), None);
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

    /// Register an additional alias key for an executor, keeping the proxy
    /// definition lookup aligned with the executor lookup so that callers using
    /// a project/task executor kind (e.g. `antigravity`) resolve the same proxy
    /// binding as callers using the executor definition id (e.g.
    /// `builtin-antigravity`). The first executor to claim a name alias wins,
    /// mirroring the precedence used when executors are registered.
    fn register_alias(&mut self, key: &str, executor: Arc<dyn Executor>, def: &ExecutorDefinition) {
        if !self.executors.contains_key(key) {
            self.executors.insert(key.to_string(), executor);
            self.definitions.insert(key.to_string(), def.clone());
        }
    }

    pub fn get(&self, id_or_name: &str) -> Option<Arc<dyn Executor>> {
        self.executors
            .get(id_or_name)
            .or_else(|| self.executors.get(&id_or_name.to_ascii_lowercase()))
            .cloned()
    }

    /// 【核心修复】：方案 A 路由逻辑：默认直连；显式 default 走默认；显式 ID 走特定代理
    pub fn resolve_proxy_for(&self, id_or_name: &str) -> Option<ProxyConfig> {
        let def = self
            .definitions
            .get(id_or_name)
            .or_else(|| self.definitions.get(&id_or_name.to_ascii_lowercase()));

        let proxy_id = def.and_then(|d| d.proxy_id.as_deref());

        match proxy_id {
            // 方案 A：未指定、空串或显式指定 "none" 时，一律直连（不走代理）
            None | Some("") | Some("none") => None,
            // 显式指定 default 时走默认代理
            Some("default") => self.default_proxy.as_ref().filter(|p| p.enabled).cloned(),
            // 显式指定特定 ID 时精确匹配
            Some(id) => self.proxies.get(id).filter(|p| p.enabled).cloned(),
        }
    }
}

/// Resolve the executable to launch for a configured executor definition.
/// Explicit `command` wins, then `executable`, then the kind name itself.
fn executor_command(def: &ExecutorDefinition) -> &str {
    if !def.command.trim().is_empty() {
        return &def.command;
    }
    if let Some(path) = def.executable.as_deref().and_then(|p| p.to_str()) {
        return path;
    }
    &def.kind
}

pub type SharedExecutorRegistry = Arc<RwLock<Arc<ExecutorRegistry>>>;

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
