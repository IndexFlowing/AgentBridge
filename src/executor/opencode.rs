use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::{Child, Command};

use crate::config::{ExecutorConfig, ExecutorMode, ProxyConfig, ALLOWED_EXECUTOR_TYPES};
use crate::executor::process::find_executable;
use crate::executor::{Executor, ExecutorError, SpawnedTask};
use crate::protocol::C2cPlan;

#[derive(Debug, Clone)]
pub struct OpenCodeExecutor {
    id: String,
    name: String,
    command: String,
    mode: ExecutorMode,
}

impl OpenCodeExecutor {
    pub fn new(id: &str, name: &str, command: &str, mode: ExecutorMode) -> Result<Self, ExecutorError> {
        let command = command.trim();
        if command.is_empty() {
            return Err(ExecutorError::InvalidCommand("command must not be empty".into()));
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
        validate_executor_type(&cfg.kind)?;
        Self::new("default", "OpenCode", &cfg.command, cfg.mode)
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn mode(&self) -> ExecutorMode {
        self.mode
    }
}

impl Executor for OpenCodeExecutor {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> &str {
        "opencode"
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
        workspace: &Path,
        proxy: Option<&ProxyConfig>,
    ) -> Result<SpawnedTask, ExecutorError> {
        plan.validate().map_err(|e| ExecutorError::Other(e.to_string()))?;
        if !workspace.is_dir() {
            return Err(ExecutorError::InvalidWorkspace(workspace.display().to_string()));
        }
        let exe = self.detect()?;
        let prompt = executor_argv_prompt();
        let child = spawn_opencode(&exe, workspace, prompt, proxy)?;
        let pid = child
            .id()
            .ok_or_else(|| ExecutorError::Spawn("OpenCode process has no pid".into()))?;
        Ok(SpawnedTask {
            child,
            pid,
            executable: exe,
        })
    }
}

pub fn validate_executor_type(kind: &str) -> Result<(), ExecutorError> {
    let kind = kind.trim().to_ascii_lowercase();
    if !ALLOWED_EXECUTOR_TYPES
        .iter()
        .any(|allowed| *allowed == kind)
    {
        return Err(ExecutorError::TypeNotAllowed(kind));
    }
    if kind != "opencode" {
        return Err(ExecutorError::TypeNotImplemented(kind));
    }
    Ok(())
}

fn executor_argv_prompt() -> &'static str {
    "Read .agentbridge/current.c2c and implement that PLAN. Stay inside this working directory. Run the TESTS. Print a short summary of changed files and test results. Do not paste source or internal reasoning."
}

fn spawn_opencode(
    exe: &Path,
    workspace: &Path,
    prompt: &str,
    proxy: Option<&ProxyConfig>,
) -> Result<Child, ExecutorError> {
    let mut cmd = Command::new(exe);
    cmd.arg("run")
        .arg("--auto")
        .arg(prompt)
        .current_dir(workspace)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .env_remove("AGENTBRIDGE_AUTH_TOKEN");

    if let Some(proxy) = proxy {
        if proxy.enabled {
            let url = proxy.url().map_err(|e| ExecutorError::Other(e.to_string()))?;
            cmd.env("HTTP_PROXY", &url)
                .env("HTTPS_PROXY", &url)
                .env("ALL_PROXY", &url)
                .env("NO_PROXY", "localhost,127.0.0.1,::1");
        }
    }

    #[cfg(windows)]
    {
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP);
    }
    #[cfg(unix)]
    {
        cmd.process_group(0);
    }

    cmd.spawn().map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            ExecutorError::NotInstalled(exe.display().to_string())
        } else {
            ExecutorError::Spawn(err.to_string())
        }
    })
}