// src/service/state.rs
//! Persisted lifecycle state for the AgentBridge background service.
//!
//! This file records the PID of the AgentBridge *server* process. It is
//! deliberately separate from `<workspace>/.agentbridge/executor.pid`, which
//! tracks a transient Executor subprocess and must never be used to control the
//! service.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const SERVICE_STATE_FILE: &str = "service.json";
pub const SERVICE_LOG_FILE: &str = "service.log";
pub const SERVICE_LOCK_FILE: &str = "service.lock";

/// On-disk record describing the running AgentBridge service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceRecord {
    /// PID of the AgentBridge server process (never an Executor PID).
    pub pid: u32,
    pub host: String,
    pub port: u16,
    pub version: String,
    pub started_at: DateTime<Utc>,
}

impl ServiceRecord {
    pub fn new(pid: u32, host: impl Into<String>, port: u16, version: impl Into<String>) -> Self {
        Self {
            pid,
            host: host.into(),
            port,
            version: version.into(),
            started_at: Utc::now(),
        }
    }

    pub fn listen_addr(&self) -> String {
        if self.host.contains(':') && !self.host.starts_with('[') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    /// Read a record from `path`. A missing or empty file means "no record".
    pub fn read(path: &Path) -> Result<Option<Self>> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
            Err(err) => {
                return Err(err).with_context(|| format!("failed to read {}", path.display()))
            }
        };
        if text.trim().is_empty() {
            return Ok(None);
        }
        let record = serde_json::from_str(&text)
            .with_context(|| format!("failed to parse service state {}", path.display()))?;
        Ok(Some(record))
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let body =
            serde_json::to_string_pretty(self).context("failed to serialize service state")?;
        fs::write(path, body).with_context(|| format!("failed to write {}", path.display()))
    }

    /// Remove the record file. Missing files are treated as success.
    pub fn clear(path: &Path) -> Result<()> {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err).with_context(|| format!("failed to remove {}", path.display())),
        }
    }
}

/// `~/.agentbridge` is the shared service directory on every platform.
pub fn service_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("cannot determine home directory")?;
    Ok(home.join(".agentbridge"))
}

pub fn default_state_path() -> Result<PathBuf> {
    Ok(service_dir()?.join(SERVICE_STATE_FILE))
}

pub fn default_log_path() -> Result<PathBuf> {
    Ok(service_dir()?.join(SERVICE_LOG_FILE))
}

pub fn default_lock_path() -> Result<PathBuf> {
    Ok(service_dir()?.join(SERVICE_LOCK_FILE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_roundtrip() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("service.json");
        let record = ServiceRecord::new(4242, "127.0.0.1", 8040, "1.0.4");
        record.write(&path).unwrap();

        let loaded = ServiceRecord::read(&path).unwrap().unwrap();
        assert_eq!(loaded, record);
        assert_eq!(loaded.pid, 4242);
        assert_eq!(loaded.listen_addr(), "127.0.0.1:8040");
    }

    #[test]
    fn missing_file_is_none_and_clear_is_idempotent() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("service.json");
        assert!(ServiceRecord::read(&path).unwrap().is_none());
        ServiceRecord::clear(&path).unwrap();
        ServiceRecord::clear(&path).unwrap();
    }

    #[test]
    fn empty_file_is_treated_as_no_record() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("service.json");
        fs::write(&path, "   \n").unwrap();
        assert!(ServiceRecord::read(&path).unwrap().is_none());
    }

    #[test]
    fn corrupt_file_reports_error_instead_of_panicking() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("service.json");
        fs::write(&path, "{not json").unwrap();
        assert!(ServiceRecord::read(&path).is_err());
    }

    #[test]
    fn ipv6_listen_addr_is_bracketed() {
        let record = ServiceRecord::new(1, "::1", 8040, "1.0.4");
        assert_eq!(record.listen_addr(), "[::1]:8040");
    }

    #[test]
    fn service_files_live_under_agentbridge_home() {
        let dir = default_state_path().unwrap();
        assert_eq!(dir.file_name().unwrap(), SERVICE_STATE_FILE);
        assert_eq!(
            dir.parent().unwrap().file_name().unwrap(),
            std::ffi::OsStr::new(".agentbridge")
        );
        assert_ne!(dir.file_name().unwrap(), "executor.pid");
    }
}
