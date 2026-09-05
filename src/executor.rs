//! OpenCode executor adapter.
//!
//! AgentBridge is not a coding agent. This module starts a local OpenCode CLI
//! process against the configured workspace, captures exit status, and returns
//! a compact result. Internal model reasoning is discarded.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use serde::Serialize;

use crate::config::{ExecutorConfig, ExecutorDefinition, ExecutorMode, ProxyConfig, ALLOWED_EXECUTOR_TYPES};
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

pub fn scan_executor(definition: &ExecutorDefinition) -> ExecutorAvailability {
    let command = definition
        .executable
        .as_deref()
        .and_then(|path| path.to_str())
        .unwrap_or(&definition.command);
    let executable = find_executable(command);
    let (version, error, status) = match executable.as_deref() {
        Some(path) => match std::process::Command::new(path)
            .arg("--version")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
        {
            Ok(output) => {
                let text = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .chain(String::from_utf8_lossy(&output.stderr).lines())
                    .map(str::trim)
                    .find(|line| !line.is_empty())
                    .map(ToOwned::to_owned);
                if output.status.success() && text.is_some() {
                    (text, None, ExecutorAvailabilityStatus::Available)
                } else {
                    let reason = text.unwrap_or_else(|| format!("{} --version returned {}", path.display(), output.status));
                    (None, Some(reason), ExecutorAvailabilityStatus::VersionProbeFailed)
                }
            }
            Err(err) => (None, Some(format!("{}: {err}", path.display())), ExecutorAvailabilityStatus::NotExecutable),
        },
        None => (None, Some(format!("{} not found on PATH or at the configured path", definition.command)), ExecutorAvailabilityStatus::NotFound),
    };
    ExecutorAvailability {
        id: definition.id.clone(),
        available: status == ExecutorAvailabilityStatus::Available,
        executable,
        version,
        error,
        status,
    }
}

pub fn common_executor_definitions() -> Vec<ExecutorDefinition> {
    [
        ("OpenCode", "opencode", "opencode"),
        ("Codex", "codex", "codex"),
        ("Claude Code", "claude", "claude"),
        ("Gemini", "gemini", "gemini"),
        ("Grok", "grok", "grok"),
    ]
    .into_iter()
    .map(|(name, kind, command)| {
        let mut definition = ExecutorDefinition::new(name.into(), kind.into(), command.into());
        definition.id = format!("builtin-{}", definition.kind);
        definition
    })
    .collect()
}

/// Merge persisted definitions with runtime candidates. A configured entry
/// wins over the matching built-in candidate, while distinct commands remain
/// visible so users can manage multiple installations of one executor kind.
pub fn executor_definitions_with_discovery(
    configured: &[ExecutorDefinition],
) -> Vec<(ExecutorDefinition, bool)> {
    let mut output = configured
        .iter()
        .cloned()
        .map(|definition| (definition, false))
        .collect::<Vec<_>>();
    for candidate in common_executor_definitions() {
        let duplicate = configured.iter().any(|definition| {
            definition.kind.eq_ignore_ascii_case(&candidate.kind)
                && definition.command.eq_ignore_ascii_case(&candidate.command)
                && definition.executable == candidate.executable
        });
        if !duplicate {
            output.push((candidate, true));
        }
    }
    output
}

#[derive(Debug, Clone)]
pub struct OpenCodeExecutor {
    command: String,
    mode: ExecutorMode,
    proxy: ProxyConfig,
}

impl OpenCodeExecutor {
    pub fn from_config(cfg: &ExecutorConfig) -> Result<Self, ExecutorError> {
        Self::from_config_with_proxy(cfg, &ProxyConfig::default())
    }

    pub fn from_config_with_proxy(
        cfg: &ExecutorConfig,
        proxy: &ProxyConfig,
    ) -> Result<Self, ExecutorError> {
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
        proxy
            .validate()
            .map_err(|e| ExecutorError::Other(e.to_string()))?;
        Ok(Self {
            command: command.to_string(),
            mode: cfg.mode,
            proxy: proxy.clone(),
        })
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn mode(&self) -> ExecutorMode {
        self.mode
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
        let child = spawn_opencode(&exe, workspace, prompt, &self.proxy)?;
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
    let raw = std::env::var("PATHEXT").unwrap_or_default();
    let mut extensions = raw
        .split(';')
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    for ext in [".com", ".exe", ".bat", ".cmd"] {
        if !extensions.iter().any(|item| item == ext) {
            extensions.push(ext.into());
        }
    }
    extensions
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

fn spawn_opencode(
    exe: &Path,
    workspace: &Path,
    prompt: &str,
    proxy: &ProxyConfig,
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
    if proxy.enabled {
        let url = proxy
            .url()
            .map_err(|e| ExecutorError::Other(e.to_string()))?;
        cmd.env("HTTP_PROXY", &url)
            .env("HTTPS_PROXY", &url)
            .env("ALL_PROXY", &url)
            .env("NO_PROXY", "localhost,127.0.0.1,::1");
    }

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

pub async fn test_proxy(proxy: &ProxyConfig) -> Result<String, ExecutorError> {
    proxy
        .validate()
        .map_err(|e| ExecutorError::Other(e.to_string()))?;
    let url = proxy
        .url()
        .map_err(|e| ExecutorError::Other(e.to_string()))?;
    let client = reqwest::Client::builder()
        .proxy(
            reqwest::Proxy::all(&url)
                .map_err(|_| ExecutorError::Other("invalid proxy settings".into()))?,
        )
        .build()
        .map_err(|_| ExecutorError::Other("could not create proxy test client".into()))?;
    let response = client
        .get("https://example.com")
        .send()
        .await
        .map_err(|_| ExecutorError::Other("proxy connection failed".into()))?;
    if response.status().is_success() {
        Ok("代理连接成功，HTTPS 访问可用".into())
    } else {
        Err(ExecutorError::Other(format!(
            "代理已连接，但 HTTPS 返回状态 {}",
            response.status().as_u16()
        )))
    }
}

/// Wait for OpenCode, honouring cancel. Returns a compact outcome (no reasoning).
pub async fn run_spawned(
    mut child: Child,
    cancel: tokio_util::sync::CancellationToken,
    mode: ExecutorMode,
) -> ExecutorOutcome {
    let stream_to_stdout = mode == ExecutorMode::Stream;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let out_buf = tokio::spawn(read_limited(stdout, stream_to_stdout));
    let err_buf = tokio::spawn(read_limited(stderr, stream_to_stdout));

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
    stream_to_stdout: bool,
) -> Vec<u8> {
    let Some(mut reader) = reader else {
        return Vec::new();
    };
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let mut at_line_start = true;
    loop {
        match reader.read(&mut chunk).await {
            Ok(0) => break,
            Ok(n) => {
                if stream_to_stdout {
                    use std::io::Write;
                    let mut stdout = std::io::stdout();
                    for line in String::from_utf8_lossy(&chunk[..n]).split_inclusive('\n') {
                        if at_line_start {
                            let _ = stdout.write_all(b"[executor] ");
                        }
                        let _ = stdout.write_all(line.as_bytes());
                        at_line_start = line.ends_with('\n');
                    }
                    if !String::from_utf8_lossy(&chunk[..n]).ends_with('\n') {
                        at_line_start = false;
                    }
                    let _ = std::io::stdout().flush();
                }
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
            mode: ExecutorMode::Silent,
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
    fn availability_requires_a_successful_version_probe() {
        let definition = ExecutorDefinition::new(
            "Git".into(),
            "opencode".into(),
            "git".into(),
        );
        let result = scan_executor(&definition);
        assert_eq!(result.status, ExecutorAvailabilityStatus::Available);
        assert!(result.available);
        assert!(result.version.is_some());

        let missing = ExecutorDefinition::new(
            "Missing".into(),
            "opencode".into(),
            "agentbridge-missing-executor".into(),
        );
        let result = scan_executor(&missing);
        assert_eq!(result.status, ExecutorAvailabilityStatus::NotFound);
        assert!(!result.available);
        assert!(result.error.is_some());
    }

    #[test]
    fn configured_executor_wins_without_duplicate_discovery_entry() {
        let mut configured = ExecutorDefinition::new("我的 OpenCode".into(), "opencode".into(), "opencode".into());
        configured.id = "stable-configured-id".into();
        let merged = executor_definitions_with_discovery(&[configured]);
        assert_eq!(merged.iter().filter(|(entry, _)| entry.kind == "opencode").count(), 1);
        assert!(!merged[0].1);
        assert_eq!(merged[0].0.id, "stable-configured-id");
    }

    #[test]
    fn strip_reasoning_drops_think_blocks() {
        let raw = "<think>\nsecret chain of thought\n</think>\ncreated TEST.md\n";
        let out = strip_reasoning(raw);
        assert!(!out.contains("chain of thought"));
        assert!(out.contains("created TEST.md"));
    }

    #[test]
    fn disabled_proxy_does_not_change_executor_environment() {
        let cfg = ExecutorConfig {
            kind: "opencode".into(),
            command: "opencode".into(),
            mode: ExecutorMode::Silent,
        };
        let exec = OpenCodeExecutor::from_config(&cfg).unwrap();
        assert!(!exec.proxy.enabled);
    }

    #[test]
    fn proxy_schemes_and_credentials_are_rendered_safely() {
        for (kind, scheme) in [
            (crate::config::ProxyKind::Http, "http"),
            (crate::config::ProxyKind::Https, "https"),
            (crate::config::ProxyKind::Socks5, "socks5h"),
        ] {
            let proxy = ProxyConfig {
                enabled: true,
                kind,
                host: "proxy.example".into(),
                port: 8080,
                username: Some("name".into()),
                password: Some("secret".into()),
            };
            let url = proxy.url().unwrap();
            assert!(url.starts_with(&format!("{scheme}://%6E%61%6D%65:")));
            assert!(!url.contains("secret"));
            assert!(url.contains("%73%65%63%72%65%74"));
        }
    }
}
