//! OpenCode executor adapter.
//!
//! AgentBridge is not a coding agent. This module starts a local OpenCode CLI
//! process against the configured workspace, captures exit status, and returns
//! a compact result. Internal model reasoning is discarded.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};

use crate::config::{ExecutorConfig, ALLOWED_EXECUTOR_TYPES};
use crate::protocol::C2cPlan;

const MAX_CAPTURE_BYTES: usize = 64 * 1024;
const MAX_SUMMARY_CHARS: usize = 2_000;

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

/// Local coding agent that may write files and run commands inside the workspace.
pub trait Executor: Send + Sync {
    fn name(&self) -> &'static str;
    fn detect(&self) -> Result<PathBuf, ExecutorError>;
    fn start_task(&self, plan: &C2cPlan, workspace: &Path) -> Result<SpawnedTask, ExecutorError>;
    fn status(&self, task_id: &str) -> Result<ExecutorSnapshot, ExecutorError>;
    fn result(&self, task_id: &str) -> Result<ExecutorSnapshot, ExecutorError>;
    fn cancel(&self, task_id: &str) -> Result<(), ExecutorError>;
}

/// Handle to a live OpenCode process. `status` / `result` / `cancel` are
/// implemented by [`crate::task::TaskRuntime`], which owns this handle.
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

#[derive(Debug, Clone)]
pub struct OpenCodeExecutor {
    command: String,
}

impl OpenCodeExecutor {
    pub fn from_config(cfg: &ExecutorConfig) -> Result<Self, ExecutorError> {
        validate_executor_type(&cfg.kind)?;
        let command = cfg.command.trim();
        if command.is_empty() {
            return Err(ExecutorError::InvalidCommand(
                "command must not be empty".into(),
            ));
        }
        if command.contains('\0') {
            return Err(ExecutorError::InvalidCommand("command contains NUL".into()));
        }
        Ok(Self {
            command: command.to_string(),
        })
    }

    pub fn command(&self) -> &str {
        &self.command
    }
}

impl Executor for OpenCodeExecutor {
    fn name(&self) -> &'static str {
        "opencode"
    }

    fn detect(&self) -> Result<PathBuf, ExecutorError> {
        find_executable(&self.command)
            .ok_or_else(|| ExecutorError::NotInstalled(self.command.clone()))
    }

    fn start_task(&self, plan: &C2cPlan, workspace: &Path) -> Result<SpawnedTask, ExecutorError> {
        plan.validate()
            .map_err(|e| ExecutorError::Other(e.to_string()))?;
        if !workspace.is_dir() {
            return Err(ExecutorError::InvalidWorkspace(
                workspace.display().to_string(),
            ));
        }
        let exe = self.detect()?;
        let prompt = executor_argv_prompt();
        let child = spawn_opencode(&exe, workspace, prompt)?;
        let pid = child
            .id()
            .ok_or_else(|| ExecutorError::Spawn("OpenCode process has no pid".into()))?;
        Ok(SpawnedTask {
            child,
            pid,
            executable: exe,
        })
    }

    fn status(&self, _task_id: &str) -> Result<ExecutorSnapshot, ExecutorError> {
        Err(ExecutorError::Other(
            "status is provided by TaskRuntime".into(),
        ))
    }

    fn result(&self, _task_id: &str) -> Result<ExecutorSnapshot, ExecutorError> {
        Err(ExecutorError::Other(
            "result is provided by TaskRuntime".into(),
        ))
    }

    fn cancel(&self, _task_id: &str) -> Result<(), ExecutorError> {
        Err(ExecutorError::Other(
            "cancel is provided by TaskRuntime".into(),
        ))
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

/// Locate `command` on PATH (Windows-aware). Absolute paths from config are allowed.
pub fn find_executable(command: &str) -> Option<PathBuf> {
    let command = command.trim();
    if command.is_empty() || command.contains('\0') {
        return None;
    }
    let path = Path::new(command);
    if path.is_absolute() {
        return existing_file(path);
    }
    if command.contains("..") {
        return None;
    }
    if path.components().count() > 1 {
        return existing_file(path);
    }

    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        for candidate in candidates_in_dir(&dir, command) {
            if let Some(found) = existing_file(&candidate) {
                return Some(found);
            }
        }
    }
    None
}

fn existing_file(path: &Path) -> Option<PathBuf> {
    if path.is_file() {
        return Some(path.to_path_buf());
    }
    #[cfg(windows)]
    {
        for ext in pathext() {
            let with_ext = if has_extension(path) {
                path.to_path_buf()
            } else {
                let mut p = path.as_os_str().to_os_string();
                p.push(&ext);
                PathBuf::from(p)
            };
            if with_ext.is_file() {
                return Some(with_ext);
            }
        }
    }
    None
}

fn candidates_in_dir(dir: &Path, command: &str) -> Vec<PathBuf> {
    let base = dir.join(command);
    let mut out = vec![base.clone()];
    #[cfg(windows)]
    {
        if !has_extension(&base) {
            for ext in pathext() {
                let mut p = base.as_os_str().to_os_string();
                p.push(&ext);
                out.push(PathBuf::from(p));
            }
        }
    }
    out
}

#[cfg(windows)]
fn has_extension(path: &Path) -> bool {
    path.extension().is_some()
}

#[cfg(windows)]
fn pathext() -> Vec<String> {
    let raw = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
    raw.split(';')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

pub fn opencode_version(command: &str) -> Option<String> {
    let exe = find_executable(command)?;
    let output = std::process::Command::new(&exe)
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(ToOwned::to_owned);
    if let Some(line) = line {
        return Some(line);
    }
    Some(exe.display().to_string())
}

/// Short argv payload. The full PLAN is in `.agentbridge/current.c2c` so Windows
/// `.cmd` wrappers are not given multiline arguments.
fn executor_argv_prompt() -> &'static str {
    "Read .agentbridge/current.c2c and implement that PLAN. Stay inside this working directory. Run the TESTS. Print a short summary of changed files and test results. Do not paste source or internal reasoning."
}

fn spawn_opencode(exe: &Path, workspace: &Path, prompt: &str) -> Result<Child, ExecutorError> {
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

    #[cfg(windows)]
    {
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP);
    }

    cmd.spawn().map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            ExecutorError::NotInstalled(exe.display().to_string())
        } else {
            ExecutorError::Spawn(err.to_string())
        }
    })
}

/// Wait for OpenCode, honouring cancel. Returns a compact outcome (no reasoning).
pub async fn run_spawned(
    mut child: Child,
    cancel: tokio_util::sync::CancellationToken,
) -> ExecutorOutcome {
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let out_buf = tokio::spawn(read_limited(stdout));
    let err_buf = tokio::spawn(read_limited(stderr));

    enum Finish {
        Status(std::io::Result<std::process::ExitStatus>),
        Cancelled,
    }

    let finish = tokio::select! {
        _ = cancel.cancelled() => {
            if let Some(pid) = child.id() {
                let _ = kill_process_tree(pid);
            }
            let _ = child.kill().await;
            Finish::Cancelled
        }
        status = child.wait() => Finish::Status(status),
    };

    if matches!(finish, Finish::Cancelled) {
        let _ = child.wait().await;
    }

    let stdout_bytes = out_buf.await.unwrap_or_default();
    let stderr_bytes = err_buf.await.unwrap_or_default();
    let stdout = String::from_utf8_lossy(&stdout_bytes);
    let stderr = String::from_utf8_lossy(&stderr_bytes);
    let stdout = strip_reasoning(&stdout);
    let stderr = strip_reasoning(&stderr);

    match finish {
        Finish::Cancelled => ExecutorOutcome {
            exit_code: None,
            summary: "Executor was cancelled.".into(),
            tests_excerpt: extract_tests_excerpt(&stdout),
            cancelled: true,
            error: None,
        },
        Finish::Status(Ok(status)) => {
            let code = status.code();
            let crashed = code.is_none() && !status.success();
            let summary = sanitize_summary(&stdout, &stderr, code, crashed);
            let error = if crashed {
                Some("OpenCode process crashed (no exit code).".into())
            } else if code.unwrap_or(0) != 0 {
                Some(format!("OpenCode exited with code {}.", code.unwrap_or(-1)))
            } else {
                None
            };
            ExecutorOutcome {
                exit_code: code,
                summary,
                tests_excerpt: extract_tests_excerpt(&stdout),
                cancelled: false,
                error,
            }
        }
        Finish::Status(Err(err)) => ExecutorOutcome {
            exit_code: None,
            summary: format!("Executor crashed: {err}"),
            tests_excerpt: extract_tests_excerpt(&stdout),
            cancelled: false,
            error: Some(err.to_string()),
        },
    }
}

async fn read_limited<R: tokio::io::AsyncRead + Unpin + Send + 'static>(
    reader: Option<R>,
) -> Vec<u8> {
    let Some(mut reader) = reader else {
        return Vec::new();
    };
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match reader.read(&mut chunk).await {
            Ok(0) => break,
            Ok(n) => {
                if buf.len() < MAX_CAPTURE_BYTES {
                    let take = n.min(MAX_CAPTURE_BYTES - buf.len());
                    buf.extend_from_slice(&chunk[..take]);
                }
            }
            Err(_) => break,
        }
    }
    buf
}

pub fn kill_process_tree(pid: u32) -> Result<(), ExecutorError> {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        Ok(())
    }
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &format!("-{pid}")])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        Ok(())
    }
}

pub fn process_is_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(windows)]
    {
        let output = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output();
        match output {
            Ok(o) => {
                let s = String::from_utf8_lossy(&o.stdout);
                s.split(',')
                    .any(|col| col.trim_matches('"') == pid.to_string())
                    || s.contains(&pid.to_string())
            }
            Err(_) => false,
        }
    }
    #[cfg(unix)]
    {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

fn strip_reasoning(text: &str) -> String {
    let mut out = String::new();
    let mut in_think = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.contains("<think>") || trimmed.contains("<thinking>") {
            in_think = true;
        }
        if in_think {
            if trimmed.contains("</think>") || trimmed.contains("</thinking>") {
                in_think = false;
            }
            continue;
        }
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if is_reasoning_event(&value) {
                continue;
            }
            if let Some(text) = json_text(&value) {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&text);
                continue;
            }
        }
        if trimmed.eq_ignore_ascii_case("thinking")
            || trimmed.starts_with("thinking:")
            || trimmed.starts_with("Reasoning:")
        {
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(line);
    }
    out
}

fn is_reasoning_event(value: &serde_json::Value) -> bool {
    let ty = value
        .get("type")
        .or_else(|| value.get("kind"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        ty.as_str(),
        "thinking" | "reasoning" | "think" | "internal" | "thought"
    )
}

fn json_text(value: &serde_json::Value) -> Option<String> {
    if let Some(s) = value.get("text").and_then(|v| v.as_str()) {
        return Some(s.to_string());
    }
    if let Some(s) = value.get("response").and_then(|v| v.as_str()) {
        return Some(s.to_string());
    }
    if let Some(s) = value
        .get("part")
        .and_then(|v| v.get("text"))
        .and_then(|v| v.as_str())
    {
        return Some(s.to_string());
    }
    None
}

fn sanitize_summary(stdout: &str, stderr: &str, exit_code: Option<i32>, crashed: bool) -> String {
    if crashed {
        return truncate_chars(
            &format!(
                "OpenCode crashed. {}",
                stderr.trim().lines().next().unwrap_or("")
            ),
            MAX_SUMMARY_CHARS,
        );
    }
    let body = stdout.trim();
    if !body.is_empty() {
        return truncate_chars(body, MAX_SUMMARY_CHARS);
    }
    let err = stderr.trim();
    if !err.is_empty() {
        return truncate_chars(err, MAX_SUMMARY_CHARS);
    }
    match exit_code {
        Some(0) => "OpenCode finished successfully.".into(),
        Some(code) => format!("OpenCode exited with code {code}."),
        None => "OpenCode finished.".into(),
    }
}

fn extract_tests_excerpt(stdout: &str) -> Option<String> {
    let mut hits = Vec::new();
    for line in stdout.lines() {
        let l = line.trim();
        let lower = l.to_ascii_lowercase();
        if lower.contains("test result:")
            || lower.contains("passed")
            || lower.contains("failed")
            || lower.contains("cargo test")
            || lower.contains("npm test")
            || lower.contains("pytest")
        {
            hits.push(l.to_string());
        }
        if hits.len() >= 8 {
            break;
        }
    }
    if hits.is_empty() {
        None
    } else {
        Some(hits.join("\n"))
    }
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_missing_binary() {
        let cfg = ExecutorConfig {
            kind: "opencode".into(),
            command: "opencode-not-installed-agentbridge-xyz".into(),
        };
        let exec = OpenCodeExecutor::from_config(&cfg).unwrap();
        let err = exec.detect().unwrap_err();
        assert!(matches!(err, ExecutorError::NotInstalled(_)));
        assert!(err.to_string().contains("not installed"));
    }

    #[test]
    fn rejects_unimplemented_types() {
        let err = validate_executor_type("codex").unwrap_err();
        assert!(matches!(err, ExecutorError::TypeNotImplemented(_)));
        let err = validate_executor_type("not-a-real-agent").unwrap_err();
        assert!(matches!(err, ExecutorError::TypeNotAllowed(_)));
        validate_executor_type("opencode").unwrap();
    }

    #[test]
    fn finds_git_on_path() {
        assert!(find_executable("git").is_some());
    }

    #[test]
    fn strip_reasoning_drops_think_blocks() {
        let raw = "<think>\nsecret chain of thought\n</think>\ncreated TEST.md\n";
        let out = strip_reasoning(raw);
        assert!(!out.contains("chain of thought"));
        assert!(out.contains("created TEST.md"));
    }
}
