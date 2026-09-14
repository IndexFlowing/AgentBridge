// src/service/manager.rs
//! Unified start/stop/restart/status lifecycle for the AgentBridge service.
//!
//! `serve` remains the single foreground entry point. This manager only adds
//! supervision on top of it: it records the server PID, prevents duplicate
//! starts, and stops the exact process it started.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{bail, Result};

use super::backend::{
    acquire_lock, HealthProbe, HttpHealthProbe, NativeBackend, ServiceBackend, SpawnSpec,
};
use super::state::{default_lock_path, default_state_path, ServiceRecord};

pub const RUNTIME_VERSION: &str = env!("CARGO_PKG_VERSION");

const DEFAULT_LOCK_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_TERM_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_KILL_TIMEOUT: Duration = Duration::from_secs(3);

/// Result of classifying the recorded service PID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceStatus {
    Running(ServiceRecord),
    /// A record exists but its PID is gone (crash / forced termination).
    Stale(ServiceRecord),
    Stopped,
}

impl ServiceStatus {
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running(_))
    }

    pub fn record(&self) -> Option<&ServiceRecord> {
        match self {
            Self::Running(record) | Self::Stale(record) => Some(record),
            Self::Stopped => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartOutcome {
    Started(ServiceRecord),
    AlreadyRunning(ServiceRecord),
    /// Something answers `/health` on the target address without a state file.
    AlreadyRunningUnmanaged {
        host: String,
        port: u16,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopOutcome {
    Stopped(u32),
    NotRunning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestartOutcome {
    pub stopped_pid: Option<u32>,
    pub start: StartOutcome,
}

pub struct ServiceManager {
    state_path: PathBuf,
    lock_path: PathBuf,
    backend: Arc<dyn ServiceBackend>,
    health: Arc<dyn HealthProbe>,
    lock_timeout: Duration,
    term_timeout: Duration,
    kill_timeout: Duration,
}

impl ServiceManager {
    /// Manager wired to the host OS and `~/.agentbridge/service.*`.
    pub fn native() -> Result<Self> {
        let state_path = default_state_path()?;
        let lock_path = default_lock_path()?;
        let log_path = super::state::default_log_path()?;
        let backend = Arc::new(NativeBackend::from_current_exe(log_path)?);
        let health = Arc::new(HttpHealthProbe::default());
        Ok(Self::with_parts(state_path, lock_path, backend, health))
    }

    pub fn with_parts(
        state_path: PathBuf,
        lock_path: PathBuf,
        backend: Arc<dyn ServiceBackend>,
        health: Arc<dyn HealthProbe>,
    ) -> Self {
        Self {
            state_path,
            lock_path,
            backend,
            health,
            lock_timeout: DEFAULT_LOCK_TIMEOUT,
            term_timeout: DEFAULT_TERM_TIMEOUT,
            kill_timeout: DEFAULT_KILL_TIMEOUT,
        }
    }

    /// Shorten timeouts; used by tests so no real service is required.
    pub fn with_timeouts(mut self, lock: Duration, term: Duration, kill: Duration) -> Self {
        self.lock_timeout = lock;
        self.term_timeout = term;
        self.kill_timeout = kill;
        self
    }

    pub fn state_path(&self) -> &Path {
        &self.state_path
    }

    pub fn log_path(&self) -> PathBuf {
        self.backend.log_path()
    }

    pub fn read_record(&self) -> Result<Option<ServiceRecord>> {
        ServiceRecord::read(&self.state_path)
    }

    /// Classify the service. PID liveness is authoritative; no SQLite involved.
    pub fn status(&self) -> Result<ServiceStatus> {
        match self.read_record()? {
            None => Ok(ServiceStatus::Stopped),
            Some(record) => {
                if self.backend.is_alive(record.pid) {
                    Ok(ServiceStatus::Running(record))
                } else {
                    Ok(ServiceStatus::Stale(record))
                }
            }
        }
    }

    /// Start `serve` in the background. Never launches a second instance.
    pub fn start(
        &self,
        host: &str,
        port: u16,
        args: Vec<String>,
        startup_timeout: Duration,
    ) -> Result<StartOutcome> {
        let _guard = acquire_lock(&self.lock_path, self.lock_timeout)?;

        match self.status()? {
            ServiceStatus::Running(record) => return Ok(StartOutcome::AlreadyRunning(record)),
            ServiceStatus::Stale(record) => {
                tracing::warn!(pid = record.pid, "removing stale service state");
                ServiceRecord::clear(&self.state_path)?;
            }
            ServiceStatus::Stopped => {}
        }

        if self.health.is_healthy(host, port) {
            return Ok(StartOutcome::AlreadyRunningUnmanaged {
                host: host.to_string(),
                port,
            });
        }

        let spec = SpawnSpec {
            args,
            log_path: self.backend.log_path(),
        };
        let pid = self.backend.spawn(&spec)?;
        let record = ServiceRecord::new(pid, host, port, RUNTIME_VERSION);
        record.write(&self.state_path)?;

        let deadline = Instant::now() + startup_timeout;
        loop {
            if self.health.is_healthy(host, port) {
                return Ok(StartOutcome::Started(record));
            }
            if !self.backend.is_alive(pid) {
                ServiceRecord::clear(&self.state_path)?;
                bail!(
                    "AgentBridge service exited during startup; check the log at {}",
                    self.log_path().display()
                );
            }
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        if self.backend.is_alive(pid) {
            Ok(StartOutcome::Started(record))
        } else {
            ServiceRecord::clear(&self.state_path)?;
            bail!(
                "AgentBridge service failed to stay up; check the log at {}",
                self.log_path().display()
            );
        }
    }

    /// Stop the recorded service. `force` overrides the ownership check.
    pub fn stop(&self, force: bool) -> Result<StopOutcome> {
        let _guard = acquire_lock(&self.lock_path, self.lock_timeout)?;

        let Some(record) = self.read_record()? else {
            return Ok(StopOutcome::NotRunning);
        };
        if !self.backend.is_alive(record.pid) {
            ServiceRecord::clear(&self.state_path)?;
            return Ok(StopOutcome::NotRunning);
        }

        let owns_endpoint = self.health.is_healthy(&record.host, record.port);
        if !owns_endpoint && !force {
            bail!(
                "PID {} is alive but the AgentBridge health check on {} failed; \
                 refusing to kill a process that may not belong to AgentBridge. \
                 Re-run with --force to override.",
                record.pid,
                record.listen_addr()
            );
        }

        self.backend.terminate(record.pid)?;
        if !self.wait_until_dead(record.pid, self.term_timeout) {
            self.backend.kill(record.pid)?;
            if !self.wait_until_dead(record.pid, self.kill_timeout) {
                bail!("failed to stop AgentBridge service (pid {})", record.pid);
            }
        }
        ServiceRecord::clear(&self.state_path)?;
        Ok(StopOutcome::Stopped(record.pid))
    }

    pub fn restart(
        &self,
        host: &str,
        port: u16,
        args: Vec<String>,
        startup_timeout: Duration,
        force: bool,
    ) -> Result<RestartOutcome> {
        let stopped_pid = match self.stop(force)? {
            StopOutcome::Stopped(pid) => Some(pid),
            StopOutcome::NotRunning => None,
        };
        let start = self.start(host, port, args, startup_timeout)?;
        Ok(RestartOutcome { stopped_pid, start })
    }

    fn wait_until_dead(&self, pid: u32, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            if !self.backend.is_alive(pid) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::Mutex;

    use super::*;
    use crate::service::backend::SpawnSpec;

    const FAST: Duration = Duration::from_millis(30);

    struct FakeBackend {
        alive: Mutex<HashSet<u32>>,
        spawned: Mutex<Vec<Vec<String>>>,
        next_pid: AtomicU32,
        log_path: PathBuf,
        die_on_spawn: AtomicBool,
        health: Arc<AtomicBool>,
        healthy_after_spawn: bool,
    }

    impl FakeBackend {
        fn new(log_path: PathBuf, health: Arc<AtomicBool>, healthy_after_spawn: bool) -> Self {
            Self {
                alive: Mutex::new(HashSet::new()),
                spawned: Mutex::new(Vec::new()),
                next_pid: AtomicU32::new(1000),
                log_path,
                die_on_spawn: AtomicBool::new(false),
                health,
                healthy_after_spawn,
            }
        }

        fn spawn_count(&self) -> usize {
            self.spawned.lock().unwrap().len()
        }
    }

    impl ServiceBackend for FakeBackend {
        fn spawn(&self, spec: &SpawnSpec) -> Result<u32> {
            let pid = self.next_pid.fetch_add(1, Ordering::SeqCst);
            if !self.die_on_spawn.load(Ordering::SeqCst) {
                self.alive.lock().unwrap().insert(pid);
            }
            if self.healthy_after_spawn {
                self.health.store(true, Ordering::SeqCst);
            }
            self.spawned.lock().unwrap().push(spec.args.clone());
            Ok(pid)
        }
        fn is_alive(&self, pid: u32) -> bool {
            self.alive.lock().unwrap().contains(&pid)
        }
        fn terminate(&self, pid: u32) -> Result<()> {
            self.alive.lock().unwrap().remove(&pid);
            self.health.store(false, Ordering::SeqCst);
            Ok(())
        }
        fn kill(&self, pid: u32) -> Result<()> {
            self.alive.lock().unwrap().remove(&pid);
            self.health.store(false, Ordering::SeqCst);
            Ok(())
        }
        fn log_path(&self) -> PathBuf {
            self.log_path.clone()
        }
    }

    struct FakeHealth {
        healthy: Arc<AtomicBool>,
    }

    impl HealthProbe for FakeHealth {
        fn is_healthy(&self, _host: &str, _port: u16) -> bool {
            self.healthy.load(Ordering::SeqCst)
        }
    }

    struct Fixture {
        _dir: tempfile::TempDir,
        manager: ServiceManager,
        backend: Arc<FakeBackend>,
        health: Arc<AtomicBool>,
    }

    fn fixture(initially_healthy: bool, healthy_after_spawn: bool) -> Fixture {
        let dir = tempfile::TempDir::new().unwrap();
        let health = Arc::new(AtomicBool::new(initially_healthy));
        let backend = Arc::new(FakeBackend::new(
            dir.path().join("service.log"),
            health.clone(),
            healthy_after_spawn,
        ));
        let probe = Arc::new(FakeHealth {
            healthy: health.clone(),
        });
        let manager = ServiceManager::with_parts(
            dir.path().join("service.json"),
            dir.path().join("service.lock"),
            backend.clone(),
            probe,
        )
        .with_timeouts(FAST, FAST, FAST);
        Fixture {
            _dir: dir,
            manager,
            backend,
            health,
        }
    }

    fn spawn_args() -> Vec<String> {
        vec!["serve".into(), "--host".into(), "127.0.0.1".into()]
    }

    #[test]
    fn status_is_stopped_without_state_file() {
        let fx = fixture(false, true);
        assert_eq!(fx.manager.status().unwrap(), ServiceStatus::Stopped);
    }

    #[test]
    fn start_records_pid_and_reports_started() {
        let fx = fixture(false, true);
        let outcome = fx
            .manager
            .start("127.0.0.1", 8040, spawn_args(), FAST)
            .unwrap();
        let StartOutcome::Started(record) = outcome else {
            panic!("expected Started, got {outcome:?}");
        };
        assert_eq!(record.pid, 1000);
        assert!(fx.backend.is_alive(record.pid));
        assert_eq!(fx.manager.read_record().unwrap().unwrap(), record);
        assert_eq!(fx.backend.spawn_count(), 1);
    }

    #[test]
    fn duplicate_start_reports_already_running_without_spawning_second() {
        let fx = fixture(false, true);
        fx.manager
            .start("127.0.0.1", 8040, spawn_args(), FAST)
            .unwrap();
        let second = fx
            .manager
            .start("127.0.0.1", 8040, spawn_args(), FAST)
            .unwrap();
        assert!(matches!(second, StartOutcome::AlreadyRunning(_)));
        assert_eq!(fx.backend.spawn_count(), 1);
    }

    #[test]
    fn stop_when_not_running_is_clear() {
        let fx = fixture(false, true);
        assert_eq!(fx.manager.stop(false).unwrap(), StopOutcome::NotRunning);
    }

    #[test]
    fn stop_terminates_recorded_pid_and_clears_state() {
        let fx = fixture(false, true);
        let StartOutcome::Started(record) = fx
            .manager
            .start("127.0.0.1", 8040, spawn_args(), FAST)
            .unwrap()
        else {
            panic!("expected Started");
        };
        assert_eq!(
            fx.manager.stop(false).unwrap(),
            StopOutcome::Stopped(record.pid)
        );
        assert!(!fx.backend.is_alive(record.pid));
        assert!(fx.manager.read_record().unwrap().is_none());
    }

    #[test]
    fn stale_pid_is_reported_and_replaced_on_next_start() {
        let fx = fixture(false, true);
        ServiceRecord::new(555_555, "127.0.0.1", 8040, "1.0.4")
            .write(fx.manager.state_path())
            .unwrap();
        assert!(matches!(
            fx.manager.status().unwrap(),
            ServiceStatus::Stale(_)
        ));
        let outcome = fx
            .manager
            .start("127.0.0.1", 8040, spawn_args(), FAST)
            .unwrap();
        let StartOutcome::Started(record) = outcome else {
            panic!("expected Started");
        };
        assert_ne!(record.pid, 555_555);
    }

    #[test]
    fn stop_refuses_unconfirmed_owner_without_force() {
        let fx = fixture(false, false);
        let record = ServiceRecord::new(1000, "127.0.0.1", 8040, "1.0.4");
        record.write(fx.manager.state_path()).unwrap();
        // PID 1000 is a real fixture process in the fake backend.
        fx.backend.alive.lock().unwrap().insert(1000);
        assert!(fx.manager.stop(false).is_err());
        assert!(fx.backend.is_alive(1000));
        assert!(fx.manager.read_record().unwrap().is_some());
    }

    #[test]
    fn stop_force_terminates_unconfirmed_owner() {
        let fx = fixture(false, false);
        let record = ServiceRecord::new(1000, "127.0.0.1", 8040, "1.0.4");
        record.write(fx.manager.state_path()).unwrap();
        fx.backend.alive.lock().unwrap().insert(1000);
        assert_eq!(fx.manager.stop(true).unwrap(), StopOutcome::Stopped(1000));
        assert!(!fx.backend.is_alive(1000));
    }

    #[test]
    fn start_refuses_when_endpoint_is_already_serving() {
        let fx = fixture(true, true);
        let outcome = fx
            .manager
            .start("127.0.0.1", 8040, spawn_args(), FAST)
            .unwrap();
        assert!(matches!(
            outcome,
            StartOutcome::AlreadyRunningUnmanaged { port: 8040, .. }
        ));
        assert_eq!(fx.backend.spawn_count(), 0);
    }

    #[test]
    fn start_fails_cleanly_when_process_dies_before_ready() {
        let fx = fixture(false, false);
        fx.backend.die_on_spawn.store(true, Ordering::SeqCst);
        let err = fx
            .manager
            .start("127.0.0.1", 8040, spawn_args(), FAST)
            .unwrap_err();
        assert!(err.to_string().contains("exited during startup"));
        assert!(fx.manager.read_record().unwrap().is_none());
    }

    #[test]
    fn restart_stops_then_starts() {
        let fx = fixture(false, true);
        let StartOutcome::Started(first) = fx
            .manager
            .start("127.0.0.1", 8040, spawn_args(), FAST)
            .unwrap()
        else {
            panic!("expected Started");
        };
        let outcome = fx
            .manager
            .restart("127.0.0.1", 8040, spawn_args(), FAST, false)
            .unwrap();
        assert_eq!(outcome.stopped_pid, Some(first.pid));
        let StartOutcome::Started(second) = outcome.start else {
            panic!("expected Started");
        };
        assert_ne!(second.pid, first.pid);
        assert_eq!(fx.backend.spawn_count(), 2);
    }

    #[test]
    fn restart_without_running_service_only_starts() {
        let fx = fixture(false, true);
        let outcome = fx
            .manager
            .restart("127.0.0.1", 8040, spawn_args(), FAST, false)
            .unwrap();
        assert_eq!(outcome.stopped_pid, None);
        assert!(matches!(outcome.start, StartOutcome::Started(_)));
    }

    #[test]
    fn health_failure_still_records_running_pid() {
        let fx = fixture(false, false);
        let outcome = fx
            .manager
            .start("127.0.0.1", 8040, spawn_args(), FAST)
            .unwrap();
        assert!(matches!(outcome, StartOutcome::Started(_)));
        assert!(fx.manager.status().unwrap().is_running());
        // PID-liveness alone keeps status running even when `/health` is quiet.
        assert!(!fx.health.load(Ordering::SeqCst));
        fx.manager.stop(true).unwrap();
    }
}
