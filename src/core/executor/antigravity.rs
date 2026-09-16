use std::path::{Path, PathBuf};

use crate::config::{ExecutorConfig, ExecutorMode, ProxyConfig};
use crate::core::agent::AgentContext;
use crate::executor::handoff::{executor_prompt, write_handoff};
use crate::executor::process::{find_executable_in, parse_command, spawn_cli};
use crate::executor::{Executor, ExecutorError, SpawnedTask};
use crate::protocol::C2cPlan;

/// Flags for the Antigravity agent CLI (`agy`, the Go binary installed by the
/// Antigravity CLI). Non-interactive execution uses headless print mode: the
/// canonical flag is the single-dash `-p` (aliases `--print` / `--prompt`),
/// which runs one prompt and exits instead of dropping into the interactive
/// TUI/IDE. The prompt is the flag's value, so `-p <prompt>` stays adjacent.
/// The old single-dash `-cli` flag was not a real argument: when the configured
/// `command` resolved to the Electron/VS Code `antigravity` launcher, that
/// token was split into one-letter switches (`-c -l -i`), which produced the
/// Chromium warnings and never reached the agent. Keep this argv in one place
/// so the launch contract stays regression-testable.
const SKIP_PERMISSIONS_FLAG: &str = "--dangerously-skip-permissions";
const PRINT_FLAG: &str = "-p";

/// Exact argv handed to the Antigravity agent CLI:
/// `agy --dangerously-skip-permissions -p <prompt>`.
/// Every element is one complete token; the print flag precedes its prompt
/// value so the CLI runs one headless turn instead of opening the TUI.
pub fn cli_args(prompt: &str) -> [&str; 3] {
    [SKIP_PERMISSIONS_FLAG, PRINT_FLAG, prompt]
}

/// Full argv for a launch: any arguments explicitly configured alongside the
/// program stay separate tokens, followed by the fixed CLI flags and prompt.
fn build_args(extra: &[String], prompt: &str) -> Vec<String> {
    let mut args = Vec::with_capacity(extra.len() + 3);
    args.extend(extra.iter().cloned());
    args.extend(cli_args(prompt).iter().map(|arg| (*arg).to_string()));
    args
}

/// Headless CLI entry points shipped by the Antigravity CLI, in preference
/// order. `agy` is the Go agent binary; `agy.exe` is its Windows form.
const CLI_PROGRAMS: &[&str] = &["agy", "agy.exe"];

/// Electron/IDE launcher names that must never be mistaken for the CLI. The
/// `antigravity` command opens the desktop app, so launching it from a headless
/// task stalls on the GUI instead of running a prompt.
const IDE_LAUNCHER_PROGRAMS: &[&str] =
    &["antigravity", "antigravity.cmd", "antigravity.bat", "antigravity.exe"];

fn is_ide_launcher(program: &str) -> bool {
    let name = Path::new(program)
        .file_name()
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    IDE_LAUNCHER_PROGRAMS.iter().any(|launcher| *launcher == name)
}

fn is_explicit_path(program: &str) -> bool {
    let trimmed = program.trim();
    Path::new(trimmed).is_absolute() || trimmed.contains('/') || trimmed.contains('\\')
}

/// Resolve the Antigravity CLI entry point for a configured program.
///
/// Antigravity installs two entry points: the headless agent CLI (`agy`) and an
/// Electron/IDE launcher (`antigravity`). Only the CLI may be launched for a
/// task, so a bare launcher name is redirected to the CLI rather than executed.
/// An explicit path is always honored as-is, so a user-chosen binary is never
/// silently substituted.
pub fn resolve_executable(program: &str) -> Option<PathBuf> {
    let dirs = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();
    resolve_executable_in(program, &dirs)
}

/// [`resolve_executable`] against an explicit search path, so the selection
/// order can be proven in tests without mutating the process environment.
pub fn resolve_executable_in(program: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    let trimmed = program.trim();
    if is_explicit_path(trimmed) {
        return find_executable_in(dirs, trimmed);
    }
    // Any bare name that is not an IDE launcher (including `agy`) resolves
    // directly; launcher names are skipped so the CLI can win.
    if !is_ide_launcher(trimmed) {
        if let Some(found) = find_executable_in(dirs, trimmed) {
            return Some(found);
        }
    }
    CLI_PROGRAMS
        .iter()
        .find_map(|candidate| find_executable_in(dirs, candidate))
}

/// Executor for the Antigravity CLI. Network access is delegated to the shared
/// Proxy abstraction via `spawn_cli`; this type never builds proxy settings.
#[derive(Debug, Clone)]
pub struct AntigravityExecutor {
    id: String,
    name: String,
    command: String,
    program: String,
    extra_args: Vec<String>,
    mode: ExecutorMode,
}

impl AntigravityExecutor {
    pub fn new(
        id: &str,
        name: &str,
        command: &str,
        mode: ExecutorMode,
    ) -> Result<Self, ExecutorError> {
        // The executable and its arguments are separated here, once, so the
        // launch path can never hand a whole command string to `Command.args`
        // (which risks per-character argv).
        let (program, extra_args) = parse_command(command)?;
        Ok(Self {
            id: id.to_string(),
            name: name.to_string(),
            command: command.trim().to_string(),
            program,
            extra_args,
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

    pub fn program(&self) -> &str {
        &self.program
    }

    pub fn extra_args(&self) -> &[String] {
        &self.extra_args
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
        // Resolution only: never launch the Antigravity binary to probe it.
        // The IDE launcher is rejected in favor of the `agy` CLI.
        resolve_executable(&self.program)
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
        let args = build_args(&self.extra_args, &prompt);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let child = spawn_cli(&exe, workspace, &arg_refs, proxy)?;
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
