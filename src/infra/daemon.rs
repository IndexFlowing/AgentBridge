// src/daemon.rs
//! Service lifecycle management for the AgentBridge background daemon.
//!
//! Controls detached background execution: start, stop, restart, and status.

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
