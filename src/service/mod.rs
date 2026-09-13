// src/service/mod.rs
//! Service lifecycle management for the AgentBridge daemon.
//!
//! The CLI is the primary entry point. `serve` runs the server in the
//! foreground; `start` / `stop` / `restart` / `status` supervise it through a
//! platform-agnostic [`ServiceManager`]. Windows Service, systemd, and launchd
//! backends can be added later by implementing [`ServiceBackend`] without
//! touching the lifecycle logic.

pub mod backend;
pub mod manager;
pub mod state;

pub use backend::{
    acquire_lock, HealthProbe, HttpHealthProbe, NativeBackend, ServiceBackend, ServiceLock,
    SpawnSpec,
};
pub use manager::{
    RestartOutcome, ServiceManager, ServiceStatus, StartOutcome, StopOutcome, RUNTIME_VERSION,
};
pub use state::{
    default_lock_path, default_log_path, default_state_path, service_dir, ServiceRecord,
    SERVICE_LOCK_FILE, SERVICE_LOG_FILE, SERVICE_STATE_FILE,
};
