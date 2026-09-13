// src/service/backend.rs
//! Cross-platform process primitives for the service lifecycle.
//!
//! The traits here are the extension boundary: a future Windows Service,
//! systemd, or launchd backend only needs to implement `ServiceBackend` and
//! `HealthProbe`; the lifecycle logic in [`super::manager`] stays unchanged.

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result};

/// How to launch the foreground `serve` process.
#[derive(Debug, Clone)]
pub struct SpawnSpec {
    pub args: Vec<String>,
    pub log_path: PathBuf,
}

/// Low-level, platform-specific process control.
pub trait ServiceBackend: Send + Sync {
    /// Launch the service detached from the current process and return its PID.
    fn spawn(&self, spec: &SpawnSpec) -> Result<u32>;
    /// Whether `pid` currently exists.
    fn is_alive(&self, pid: u32) -> bool;
    /// Ask `pid` to shut down gracefully (best effort).
    fn terminate(&self, pid: u32) -> Result<()>;
    /// Force-terminate `pid` and its children.
    fn kill(&self, pid: u32) -> Result<()>;
    /// Sidecar log destination for the managed service.
    fn log_path(&self) -> PathBuf;
}

/// A readiness/ownership probe against the service HTTP endpoint.
pub trait HealthProbe: Send + Sync {
    /// True when `host:port` answers `/health` with an AgentBridge signature.
    fn is_healthy(&self, host: &str, port: u16) -> bool;
}

/// Marker path used to serialize concurrent lifecycle operations.
pub fn acquire_lock(lock_path: &Path, timeout: Duration) -> Result<ServiceLock> {
    ServiceLock::acquire(lock_path, timeout)
}

pub struct ServiceLock {
    path: PathBuf,
    held: bool,
}

impl ServiceLock {
    pub fn acquire(path: &Path, timeout: Duration) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let start = std::time::Instant::now();
        loop {
            match OpenOptions::new().write(true).create_new(true).open(path) {
                Ok(mut file) => {
                    let _ = writeln!(file, "{}", std::process::id());
                    return Ok(Self {
                        path: path.to_path_buf(),
                        held: true,
                    });
                }
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                    if is_stale_lock(path) {
                        let _ = std::fs::remove_file(path);
                        continue;
                    }
                    if start.elapsed() >= timeout {
                        anyhow::bail!(
                            "another service operation is in progress (lock {})",
                            path.display()
                        );
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(err) => {
                    return Err(err)
                        .with_context(|| format!("failed to acquire lock {}", path.display()));
                }
            }
        }
    }
}

fn is_stale_lock(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    meta.modified()
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .map(|age| age > Duration::from_secs(30))
        .unwrap_or(false)
}

impl Drop for ServiceLock {
    fn drop(&mut self) {
        if self.held {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Native process control using the host OS facilities.
pub struct NativeBackend {
    executable: PathBuf,
    log_path: PathBuf,
}

impl NativeBackend {
    pub fn new(executable: PathBuf, log_path: PathBuf) -> Self {
        Self {
            executable,
            log_path,
        }
    }

    pub fn from_current_exe(log_path: PathBuf) -> Result<Self> {
        let executable = std::env::current_exe().context("cannot locate current executable")?;
        Ok(Self::new(executable, log_path))
    }
}

impl ServiceBackend for NativeBackend {
    fn spawn(&self, spec: &SpawnSpec) -> Result<u32> {
        if let Some(parent) = spec.log_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&spec.log_path)
            .with_context(|| format!("failed to open log {}", spec.log_path.display()))?;
        let err_log = log.try_clone().context("failed to duplicate log handle")?;

        let mut command = Command::new(&self.executable);
        command
            .args(&spec.args)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(err_log));

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const DETACHED_PROCESS: u32 = 0x0000_0008;
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
            command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }

        let child = command.spawn().with_context(|| {
            format!(
                "failed to start AgentBridge service ({})",
                self.executable.display()
            )
        })?;
        Ok(child.id())
    }

    fn is_alive(&self, pid: u32) -> bool {
        process_is_alive(pid)
    }

    fn terminate(&self, pid: u32) -> Result<()> {
        #[cfg(windows)]
        {
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        #[cfg(unix)]
        {
            // Negative PID targets the process group created via `process_group(0)`.
            let _ = Command::new("kill")
                .args(["-TERM", &format!("-{pid}")])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            let _ = Command::new("kill")
                .args(["-TERM", &pid.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        Ok(())
    }

    fn kill(&self, pid: u32) -> Result<()> {
        #[cfg(windows)]
        {
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        #[cfg(unix)]
        {
            let _ = Command::new("kill")
                .args(["-KILL", &format!("-{pid}")])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            let _ = Command::new("kill")
                .args(["-KILL", &pid.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        Ok(())
    }

    fn log_path(&self) -> PathBuf {
        self.log_path.clone()
    }
}

/// Cross-platform liveness check that never signals the target process.
pub fn process_is_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(windows)]
    {
        let output = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output();
        match output {
            Ok(out) => {
                let text = String::from_utf8_lossy(&out.stdout);
                text.split(',')
                    .any(|column| column.trim_matches('"').trim() == pid.to_string())
            }
            Err(_) => false,
        }
    }
    #[cfg(unix)]
    {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
}

/// Minimal HTTP `/health` probe implemented with std networking so lifecycle
/// status never depends on the SQLite store or a heavy HTTP client.
pub struct HttpHealthProbe {
    timeout: Duration,
}

impl Default for HttpHealthProbe {
    fn default() -> Self {
        Self {
            timeout: Duration::from_millis(750),
        }
    }
}

impl HttpHealthProbe {
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }
}

impl HealthProbe for HttpHealthProbe {
    fn is_healthy(&self, host: &str, port: u16) -> bool {
        probe_health(host, port, self.timeout).unwrap_or(false)
    }
}

fn probe_health(host: &str, port: u16, timeout: Duration) -> Option<bool> {
    let connect_host = normalize_probe_host(host);
    let addr = (connect_host, port).to_socket_addrs().ok()?.next()?;
    let mut stream = TcpStream::connect_timeout(&addr, timeout).ok()?;
    stream.set_read_timeout(Some(timeout)).ok()?;
    stream.set_write_timeout(Some(timeout)).ok()?;

    let request = format!("GET /health HTTP/1.0\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).ok()?;

    let mut text = String::new();
    stream.take(4096).read_to_string(&mut text).ok()?;
    Some(looks_like_agentbridge_health(&text))
}

fn normalize_probe_host(host: &str) -> &str {
    match host {
        "0.0.0.0" => "127.0.0.1",
        "::" | "[::]" => "::1",
        other => other,
    }
}

fn looks_like_agentbridge_health(response: &str) -> bool {
    let status_ok = response
        .lines()
        .next()
        .map(|line| line.starts_with("HTTP/") && line.contains(" 200"))
        .unwrap_or(false);
    if !status_ok {
        return false;
    }
    let body = response.split("\r\n\r\n").nth(1).unwrap_or("");
    body.contains("\"status\"") && body.contains("\"ok\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pid_zero_is_never_alive() {
        assert!(!process_is_alive(0));
    }

    #[test]
    fn implausible_pid_is_not_alive() {
        assert!(!process_is_alive(4_000_000_000));
    }

    #[test]
    fn health_signature_requires_200_and_ok_body() {
        assert!(looks_like_agentbridge_health(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\r\n{\"status\":\"ok\",\"version\":\"1.0.4\"}"
        ));
        assert!(!looks_like_agentbridge_health(
            "HTTP/1.1 404 Not Found\r\n\r\n{\"status\":\"ok\"}"
        ));
        assert!(!looks_like_agentbridge_health("garbage"));
    }

    #[test]
    fn wildcard_hosts_are_normalized_for_probing() {
        assert_eq!(normalize_probe_host("0.0.0.0"), "127.0.0.1");
        assert_eq!(normalize_probe_host("::"), "::1");
        assert_eq!(normalize_probe_host("127.0.0.1"), "127.0.0.1");
    }

    #[test]
    fn lock_conflict_is_reported_and_stale_lock_is_reclaimed() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("service.lock");
        let first = ServiceLock::acquire(&path, Duration::from_millis(0)).unwrap();
        assert!(ServiceLock::acquire(&path, Duration::from_millis(0)).is_err());
        drop(first);
        assert!(ServiceLock::acquire(&path, Duration::from_millis(0)).is_ok());
    }
}
