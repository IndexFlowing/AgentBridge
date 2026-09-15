use std::path::{Path, PathBuf};

use crate::config::{ExecutorConfig, ExecutorMode, ProxyConfig};
use crate::core::agent::AgentContext;
use crate::executor::handoff::{executor_prompt, write_handoff};
use crate::executor::process::{find_executable, spawn_cli};
use crate::executor::{Executor, ExecutorError, SpawnedTask};
use crate::protocol::C2cPlan;

/// Flags of the Antigravity agent backend that expose a non-interactive,
/// single-prompt CLI (`language_server -cli -print <prompt>`). Verified against
/// a local Antigravity install; the configured `command` may point at the
/// language server binary.
const CLI_FLAG: &str = "-cli";
const SKIP_PERMISSIONS_FLAG: &str = "-dangerously-skip-permissions";
const PRINT_FLAG: &str = "-print";

/// Executor for the Antigravity CLI. Network access is delegated to the shared
/// Proxy abstraction via `spawn_cli`; this type never builds proxy settings.
#[derive(Debug, Clone)]
pub struct AntigravityExecutor {
    id: String,
    name: String,
    command: String,
    mode: ExecutorMode,
}

impl AntigravityExecutor {
    pub fn new(
        id: &str,
        name: &str,
        command: &str,
        mode: ExecutorMode,
    ) -> Result<Self, ExecutorError> {
        let command = command.trim();
        if command.is_empty() {
            return Err(ExecutorError::InvalidCommand(
                "command must not be empty".into(),
            ));
        }
        if command.contains('\0') {
            return Err(ExecutorError::InvalidCommand("command contains NUL".into()));
        }
        Ok(Self {
            id: id.to_string(),
            name: name.to_string(),
            command: command.to_string(),
            mode,
        })
    }

    pub fn from_config(cfg: &ExecutorConfig) -> Result<Self, ExecutorError> {
        if !cfg.kind.eq_ignore_ascii_case("antigravity") {
            return Err(ExecutorError::TypeNotImplemented(cfg.kind.clone()));
        }
        Self::new("default", "Antigravity", &cfg.command, cfg.mode)
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn mode(&self) -> ExecutorMode {
        self.mode
    }
}

impl Executor for AntigravityExecutor {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> &str {
        "antigravity"
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn detect(&self) -> Result<PathBuf, ExecutorError> {
        find_executable(&self.command)
            .ok_or_else(|| ExecutorError::NotInstalled(self.command.clone()))
    }

    fn start_task(
        &self,
        plan: &C2cPlan,
        context: &AgentContext,
        workspace: &Path,
        proxy: Option<&ProxyConfig>,
    ) -> Result<SpawnedTask, ExecutorError> {
        plan.validate()
            .map_err(|e| ExecutorError::Other(e.to_string()))?;
        if !workspace.is_dir() {
            return Err(ExecutorError::InvalidWorkspace(
                workspace.display().to_string(),
            ));
        }
        let exe = self.detect()?;
        let handoff = write_handoff(plan, context)?;
        let prompt = executor_prompt(&handoff);
        let child = spawn_cli(
            &exe,
            workspace,
            &[CLI_FLAG, SKIP_PERMISSIONS_FLAG, PRINT_FLAG, prompt.as_str()],
            proxy,
        )?;
        let pid = child
            .id()
            .ok_or_else(|| ExecutorError::Spawn("Antigravity process has no pid".into()))?;
        Ok(SpawnedTask {
            child,
            pid,
            executable: exe,
        })
    }
}
