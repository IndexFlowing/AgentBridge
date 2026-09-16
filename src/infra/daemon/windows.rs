// src/infra/daemon/windows.rs
//! Windows Service (SCM / NSSM) integration for AgentBridge.
//!
//! Provides service installation, uninstallation, lifecycle management (start, stop,
//! restart, status), and SCM query parsing for production Windows deployments.

use std::path::{Path, PathBuf};
use std::time::Duration;
use anyhow::{bail, Context, Result};

use super::backend::{HealthProbe, HttpHealthProbe};
use super::state::default_log_path;

pub const DEFAULT_SERVICE_NAME: &str = "AgentBridge";
pub const DEFAULT_DISPLAY_NAME: &str = "AgentBridge MCP Gateway Service";
pub const DEFAULT_DESCRIPTION: &str = "AgentBridge MCP Access to Local Coding Workspace";

/// Windows Service Control Manager (SCM) service state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScmState {
    Running,
    Stopped,
    StartPending,
    StopPending,
    Paused,
    NotFound,
    Unknown,
}

impl ScmState {
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running)
    }

    pub fn is_stopped(&self) -> bool {
        matches!(self, Self::Stopped | Self::NotFound)
    }
}

impl std::fmt::Display for ScmState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Running => write!(f, "RUNNING"),
            Self::Stopped => write!(f, "STOPPED"),
            Self::StartPending => write!(f, "START_PENDING"),
            Self::StopPending => write!(f, "STOP_PENDING"),
            Self::Paused => write!(f, "PAUSED"),
            Self::NotFound => write!(f, "NOT_FOUND"),
            Self::Unknown => write!(f, "UNKNOWN"),
        }
    }
}

/// Status of a Windows Service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsServiceStatus {
    pub service_name: String,
    pub installed: bool,
    pub state: ScmState,
    pub pid: Option<u32>,
    pub healthy: Option<bool>,
}

/// Specifications for installing AgentBridge as a Windows Service.
#[derive(Debug, Clone)]
pub struct WindowsServiceInstallSpec {
    pub service_name: String,
    pub display_name: String,
    pub description: String,
    pub config_path: Option<PathBuf>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub nssm_path: Option<PathBuf>,
    pub auto_start: bool,
}

impl Default for WindowsServiceInstallSpec {
    fn default() -> Self {
        Self {
            service_name: DEFAULT_SERVICE_NAME.to_string(),
            display_name: DEFAULT_DISPLAY_NAME.to_string(),
            description: DEFAULT_DESCRIPTION.to_string(),
            config_path: None,
            host: None,
            port: None,
            nssm_path: None,
            auto_start: true,
        }
    }
}

/// Parse the output of `sc.exe queryex <service>` into ScmState and PID.
pub fn parse_sc_query_output(output: &str) -> (ScmState, Option<u32>) {
    let lower = output.to_lowercase();
    if lower.contains("1060")
        || lower.contains("does not exist")
        || lower.contains("指定的服务未安装")
        || lower.contains("未安装")
    {
        return (ScmState::NotFound, None);
    }

    let mut state = ScmState::Unknown;
    let mut pid = None;

    for line in output.lines() {
        let trimmed = line.trim();
        let upper = trimmed.to_uppercase();

        if upper.starts_with("STATE") {
            if upper.contains("RUNNING") || upper.contains(" 4 ") {
                state = ScmState::Running;
            } else if upper.contains("STOPPED") || upper.contains(" 1 ") {
                state = ScmState::Stopped;
            } else if upper.contains("START_PENDING") || upper.contains(" 2 ") {
                state = ScmState::StartPending;
            } else if upper.contains("STOP_PENDING") || upper.contains(" 3 ") {
                state = ScmState::StopPending;
            } else if upper.contains("PAUSED") || upper.contains(" 7 ") {
                state = ScmState::Paused;
            }
        } else if upper.starts_with("PID") {
            if let Some((_, val)) = trimmed.split_once(':') {
                if let Ok(num) = val.trim().parse::<u32>() {
                    if num > 0 {
                        pid = Some(num);
                    }
                }
            }
        }
    }

    (state, pid)
}

/// Parse NSSM status command output into ScmState.
pub fn parse_nssm_status(output: &str) -> ScmState {
    let trimmed = output.trim().to_uppercase();
    if trimmed.contains("SERVICE_RUNNING") {
        ScmState::Running
    } else if trimmed.contains("SERVICE_STOPPED") {
        ScmState::Stopped
    } else if trimmed.contains("SERVICE_START_PENDING") {
        ScmState::StartPending
    } else if trimmed.contains("SERVICE_STOP_PENDING") {
        ScmState::StopPending
    } else if trimmed.contains("SERVICE_PAUSED") {
        ScmState::Paused
    } else if trimmed.contains("CAN'T OPEN SERVICE")
        || trimmed.contains("OPENSERVICE()")
        || trimmed.contains("NOT_FOUND")
    {
        ScmState::NotFound
    } else {
        ScmState::Unknown
    }
}

/// Build arguments for the `serve` command line when running under a Windows service.
pub fn build_serve_args(spec: &WindowsServiceInstallSpec) -> Vec<String> {
    let mut args = vec!["serve".to_string()];
    if let Some(config) = &spec.config_path {
        args.push("--config".to_string());
        args.push(config.to_string_lossy().to_string());
    }
    if let Some(host) = &spec.host {
        args.push("--host".to_string());
        args.push(host.clone());
    }
    if let Some(port) = spec.port {
        args.push("--port".to_string());
        args.push(port.to_string());
    }
    args
}

/// Helper to discover the nssm.exe binary on the system.
pub fn find_nssm_executable(custom: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = custom {
        if path.is_file() {
            return Ok(path.to_path_buf());
        }
        bail!("specified nssm binary does not exist: {}", path.display());
    }

    // 1. Check PATH
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join("nssm.exe");
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    // 2. Check standard installation directories
    let candidates = [
        PathBuf::from(r"C:\Program Files\nssm\nssm.exe"),
        PathBuf::from(r"C:\Program Files (x86)\nssm\nssm.exe"),
        PathBuf::from(r"D:\Program Files\nssm\nssm.exe"),
        PathBuf::from(r"D:\Program Files (x86)\nssm\nssm.exe"),
        PathBuf::from(r"C:\ProgramData\chocolatey\bin\nssm.exe"),
    ];
    for c in &candidates {
        if c.is_file() {
            return Ok(c.clone());
        }
    }

    // 3. Check Scoop installation
    if let Some(home) = dirs::home_dir() {
        let scoop_shims = home.join(r"scoop\shims\nssm.exe");
        if scoop_shims.is_file() {
            return Ok(scoop_shims);
        }
        let scoop_app = home.join(r"scoop\apps\nssm\current\nssm.exe");
        if scoop_app.is_file() {
            return Ok(scoop_app);
        }
    }

    // 4. Check adjacent to current executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let local = dir.join("nssm.exe");
            if local.is_file() {
                return Ok(local);
            }
        }
    }

    bail!(
        "NSSM (Non-Sucking Service Manager) was not found in PATH or standard locations.\n\
         Please install NSSM using one of the following commands in an Administrator terminal:\n\
           winget install nssm\n\
           choco install nssm\n\
           scoop install nssm\n\
         Or download from https://nssm.cc/download and place nssm.exe in your PATH."
    );
}

/// Unified manager for Windows Service deployment.
#[derive(Debug, Clone)]
pub struct WindowsServiceManager {
    service_name: String,
    custom_nssm: Option<PathBuf>,
}

impl Default for WindowsServiceManager {
    fn default() -> Self {
        Self::new(DEFAULT_SERVICE_NAME)
    }
}

impl WindowsServiceManager {
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
            custom_nssm: None,
        }
    }

    pub fn with_nssm(mut self, nssm: PathBuf) -> Self {
        self.custom_nssm = Some(nssm);
        self
    }

    pub fn default_service_name() -> &'static str {
        DEFAULT_SERVICE_NAME
    }

    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    pub fn log_path(&self) -> PathBuf {
        default_log_path().unwrap_or_else(|_| PathBuf::from("service.log"))
    }
}

#[cfg(windows)]
impl WindowsServiceManager {
    pub fn find_nssm(&self) -> Result<PathBuf> {
        find_nssm_executable(self.custom_nssm.as_deref())
    }

    pub fn is_installed(&self) -> Result<bool> {
        let (state, _) = self.query_scm_status()?;
        Ok(state != ScmState::NotFound && state != ScmState::Unknown)
    }

    pub fn status(&self) -> Result<WindowsServiceStatus> {
        self.status_with_probe(None, None)
    }

    pub fn status_with_probe(
        &self,
        host: Option<&str>,
        port: Option<u16>,
    ) -> Result<WindowsServiceStatus> {
        let (state, pid) = self.query_scm_status()?;
        let installed = state != ScmState::NotFound;

        let healthy = if state == ScmState::Running {
            let h = host.unwrap_or("127.0.0.1");
            let p = port.unwrap_or(8040);
            let probe = HttpHealthProbe::default();
            Some(probe.is_healthy(h, p))
        } else {
            None
        };

        Ok(WindowsServiceStatus {
            service_name: self.service_name.clone(),
            installed,
            state,
            pid,
            healthy,
        })
    }

    fn query_scm_status(&self) -> Result<(ScmState, Option<u32>)> {
        use std::process::Command;

        let output = Command::new("sc.exe")
            .args(["queryex", &self.service_name])
            .output();

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);
                let full = format!("{}\n{}", stdout, stderr);
                let (state, pid) = parse_sc_query_output(&full);
                if state != ScmState::Unknown {
                    return Ok((state, pid));
                }
                // Fallback to nssm status if sc output was unrecognized
                if let Ok(nssm) = self.find_nssm() {
                    if let Ok(nssm_out) =
                        Command::new(&nssm).args(["status", &self.service_name]).output()
                    {
                        let text = String::from_utf8_lossy(&nssm_out.stdout);
                        let nssm_state = parse_nssm_status(&text);
                        return Ok((nssm_state, pid));
                    }
                }
                Ok((state, pid))
            }
            Err(e) => {
                if let Ok(nssm) = self.find_nssm() {
                    let nssm_out = Command::new(&nssm)
                        .args(["status", &self.service_name])
                        .output()
                        .context("failed to query service status via nssm")?;
                    let text = String::from_utf8_lossy(&nssm_out.stdout);
                    Ok((parse_nssm_status(&text), None))
                } else {
                    Err(e).context("failed to query service status via sc.exe")
                }
            }
        }
    }

    pub fn install(&self, spec: &WindowsServiceInstallSpec) -> Result<()> {
        use std::process::Command;

        let nssm = self.find_nssm()?;
        let (state, _) = self.query_scm_status()?;
        if state != ScmState::NotFound && state != ScmState::Unknown {
            bail!(
                "Windows service '{}' is already installed. Use `agentbridge service uninstall` first.",
                self.service_name
            );
        }

        let exe_path =
            std::env::current_exe().context("cannot determine current executable path")?;
        let args = build_serve_args(spec);
        let args_str = args.join(" ");

        // 1. Register service via NSSM
        let install_status = Command::new(&nssm)
            .args([
                "install",
                &self.service_name,
                &exe_path.to_string_lossy(),
                &args_str,
            ])
            .output()
            .with_context(|| format!("failed to run nssm install with {}", nssm.display()))?;

        if !install_status.status.success() {
            let stderr = String::from_utf8_lossy(&install_status.stderr);
            let stdout = String::from_utf8_lossy(&install_status.stdout);
            let combined = format!("{} {}", stdout, stderr);
            if combined.contains("Access is denied")
                || combined.contains("拒绝访问")
                || combined.contains("OpenSCManager")
            {
                bail!(
                    "Administrator privileges required: Windows service operations require elevated permissions. \
                     Please run the command from an Administrator terminal."
                );
            }
            bail!("failed to install service via nssm: {}", combined.trim());
        }

        // 2. Set AppDirectory
        let work_dir = spec
            .config_path
            .as_ref()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .or_else(|| exe_path.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."));
        let _ = Command::new(&nssm)
            .args([
                "set",
                &self.service_name,
                "AppDirectory",
                &work_dir.to_string_lossy(),
            ])
            .status();

        // 3. Set AppStdout and AppStderr
        let log_path = self.log_path();
        if let Some(parent) = log_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let log_str = log_path.to_string_lossy();
        let _ = Command::new(&nssm)
            .args(["set", &self.service_name, "AppStdout", &log_str])
            .status();
        let _ = Command::new(&nssm)
            .args(["set", &self.service_name, "AppStderr", &log_str])
            .status();

        // 4. Set Log rotation: max 10MB
        let _ = Command::new(&nssm)
            .args(["set", &self.service_name, "AppRotateFiles", "1"])
            .status();
        let _ = Command::new(&nssm)
            .args(["set", &self.service_name, "AppRotateBytes", "10485760"])
            .status();

        // 5. DisplayName and Description
        let _ = Command::new(&nssm)
            .args(["set", &self.service_name, "DisplayName", &spec.display_name])
            .status();
        let _ = Command::new(&nssm)
            .args(["set", &self.service_name, "Description", &spec.description])
            .status();

        // 6. Startup type
        let startup = if spec.auto_start {
            "SERVICE_AUTO_START"
        } else {
            "SERVICE_DEMAND_START"
        };
        let _ = Command::new(&nssm)
            .args(["set", &self.service_name, "Start", startup])
            .status();

        Ok(())
    }

    pub fn uninstall(&self) -> Result<()> {
        use std::process::Command;

        let (state, _) = self.query_scm_status()?;
        if state == ScmState::NotFound {
            bail!("Windows service '{}' is not installed.", self.service_name);
        }

        // Stop first if running
        let _ = self.stop(Duration::from_secs(5));

        // Try nssm remove first
        if let Ok(nssm) = self.find_nssm() {
            let out = Command::new(&nssm)
                .args(["remove", &self.service_name, "confirm"])
                .output();
            if let Ok(out) = out {
                if out.status.success() {
                    return Ok(());
                }
                let combined = format!(
                    "{} {}",
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                );
                if combined.contains("Access is denied") || combined.contains("拒绝访问") {
                    bail!("Administrator privileges required to uninstall service.");
                }
            }
        }

        // Fallback to sc delete
        let out = Command::new("sc.exe")
            .args(["delete", &self.service_name])
            .output()
            .context("failed to delete service via sc.exe")?;

        if !out.status.success() {
            let combined = format!(
                "{} {}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            if combined.contains("Access is denied") || combined.contains("拒绝访问") {
                bail!("Administrator privileges required to uninstall service.");
            }
            bail!("failed to uninstall service: {}", combined.trim());
        }

        Ok(())
    }

    pub fn start(&self, timeout: Duration, host: &str, port: u16) -> Result<()> {
        use std::process::Command;

        let (state, _) = self.query_scm_status()?;
        if state == ScmState::NotFound {
            bail!(
                "Windows service '{}' is not installed. Run `agentbridge service install` first.",
                self.service_name
            );
        }
        if state == ScmState::Running {
            return Ok(());
        }

        let started = if let Ok(nssm) = self.find_nssm() {
            let out = Command::new(&nssm).args(["start", &self.service_name]).output();
            out.map(|o| o.status.success()).unwrap_or(false)
        } else {
            false
        };

        if !started {
            let out = Command::new("net")
                .args(["start", &self.service_name])
                .output()
                .context("failed to execute net start")?;
            if !out.status.success() {
                let combined = format!(
                    "{} {}",
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                );
                if combined.contains("Access is denied") || combined.contains("拒绝访问") {
                    bail!("Administrator privileges required to start Windows service.");
                }
                bail!("failed to start service: {}", combined.trim());
            }
        }

        // Wait for service to reach Running and probe health
        let deadline = std::time::Instant::now() + timeout;
        let probe = HttpHealthProbe::default();
        loop {
            let (st, _) = self.query_scm_status()?;
            if st == ScmState::Running {
                if probe.is_healthy(host, port) {
                    return Ok(());
                }
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        let (st, _) = self.query_scm_status()?;
        if st == ScmState::Running {
            Ok(())
        } else {
            bail!(
                "service failed to start within timeout; check the log at {}",
                self.log_path().display()
            );
        }
    }

    pub fn stop(&self, timeout: Duration) -> Result<()> {
        use std::process::Command;

        let (state, _) = self.query_scm_status()?;
        if state == ScmState::NotFound || state == ScmState::Stopped {
            return Ok(());
        }

        let stopped = if let Ok(nssm) = self.find_nssm() {
            let out = Command::new(&nssm).args(["stop", &self.service_name]).output();
            out.map(|o| o.status.success()).unwrap_or(false)
        } else {
            false
        };

        if !stopped {
            let out = Command::new("net")
                .args(["stop", &self.service_name])
                .output()
                .context("failed to execute net stop")?;
            if !out.status.success() {
                let combined = format!(
                    "{} {}",
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                );
                if combined.contains("Access is denied") || combined.contains("拒绝访问") {
                    bail!("Administrator privileges required to stop Windows service.");
                }
                bail!("failed to stop service: {}", combined.trim());
            }
        }

        let deadline = std::time::Instant::now() + timeout;
        loop {
            let (st, _) = self.query_scm_status()?;
            if st == ScmState::Stopped {
                return Ok(());
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        Ok(())
    }

    pub fn restart(&self, timeout: Duration, host: &str, port: u16) -> Result<()> {
        self.stop(timeout)?;
        std::thread::sleep(Duration::from_millis(200));
        self.start(timeout, host, port)
    }
}

#[cfg(not(windows))]
impl WindowsServiceManager {
    pub fn find_nssm(&self) -> Result<PathBuf> {
        bail!("Windows services and NSSM are only supported on Windows.");
    }

    pub fn is_installed(&self) -> Result<bool> {
        Ok(false)
    }

    pub fn status(&self) -> Result<WindowsServiceStatus> {
        Ok(WindowsServiceStatus {
            service_name: self.service_name.clone(),
            installed: false,
            state: ScmState::NotFound,
            pid: None,
            healthy: None,
        })
    }

    pub fn status_with_probe(
        &self,
        _host: Option<&str>,
        _port: Option<u16>,
    ) -> Result<WindowsServiceStatus> {
        self.status()
    }

    pub fn install(&self, _spec: &WindowsServiceInstallSpec) -> Result<()> {
        bail!("Windows service installation is only supported on Windows.");
    }

    pub fn uninstall(&self) -> Result<()> {
        bail!("Windows service uninstallation is only supported on Windows.");
    }

    pub fn start(&self, _timeout: Duration, _host: &str, _port: u16) -> Result<()> {
        bail!("Windows service management is only supported on Windows.");
    }

    pub fn stop(&self, _timeout: Duration) -> Result<()> {
        bail!("Windows service management is only supported on Windows.");
    }

    pub fn restart(&self, _timeout: Duration, _host: &str, _port: u16) -> Result<()> {
        bail!("Windows service management is only supported on Windows.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sc_query_running_with_pid() {
        let sample = r#"
SERVICE_NAME: AgentBridge 
        TYPE               : 10  WIN32_OWN_PROCESS  
        STATE              : 4  RUNNING 
                                (STOPPABLE, NOT_PAUSABLE, ACCEPTS_SHUTDOWN)
        WIN32_EXIT_CODE    : 0  (0x0)
        SERVICE_EXIT_CODE  : 0  (0x0)
        CHECKPOINT         : 0x0
        WAIT_HINT          : 0x0
        PID                : 3500
        FLAGS              : 
"#;
        let (state, pid) = parse_sc_query_output(sample);
        assert_eq!(state, ScmState::Running);
        assert_eq!(pid, Some(3500));
    }

    #[test]
    fn parse_sc_query_stopped() {
        let sample = r#"
SERVICE_NAME: AgentBridge 
        TYPE               : 10  WIN32_OWN_PROCESS  
        STATE              : 1  STOPPED 
        WIN32_EXIT_CODE    : 0  (0x0)
        SERVICE_EXIT_CODE  : 0  (0x0)
        CHECKPOINT         : 0x0
        WAIT_HINT          : 0x0
        PID                : 0
        FLAGS              : 
"#;
        let (state, pid) = parse_sc_query_output(sample);
        assert_eq!(state, ScmState::Stopped);
        assert_eq!(pid, None);
    }

    #[test]
    fn parse_sc_query_not_found() {
        let sample = "[SC] EnumQueryServicesStatus:OpenService 失败 1060:\n\n指定的服务未安装。\n";
        let (state, pid) = parse_sc_query_output(sample);
        assert_eq!(state, ScmState::NotFound);
        assert_eq!(pid, None);
    }

    #[test]
    fn parse_nssm_status_strings() {
        assert_eq!(parse_nssm_status("SERVICE_RUNNING\n"), ScmState::Running);
        assert_eq!(parse_nssm_status("SERVICE_STOPPED\n"), ScmState::Stopped);
        assert_eq!(parse_nssm_status("SERVICE_PAUSED\n"), ScmState::Paused);
        assert_eq!(
            parse_nssm_status("Can't open service!\n"),
            ScmState::NotFound
        );
    }

    #[test]
    fn build_serve_args_includes_config_and_ports() {
        let mut spec = WindowsServiceInstallSpec::default();
        spec.config_path = Some(PathBuf::from("C:\\custom\\config.toml"));
        spec.host = Some("0.0.0.0".to_string());
        spec.port = Some(8080);

        let args = build_serve_args(&spec);
        assert_eq!(
            args,
            vec![
                "serve",
                "--config",
                "C:\\custom\\config.toml",
                "--host",
                "0.0.0.0",
                "--port",
                "8080"
            ]
        );
    }
}
