//! Shared test helpers for the SQLite-backed service architecture.
#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Arc;

use agentbridge::config::Config;
use agentbridge::projects::{ProjectEntry, ProjectHub};
use agentbridge::storage::Storage;
use tempfile::TempDir;

/// Create a temp directory that is intentionally leaked for the lifetime of the
/// test process, so the SQLite file stays alive while the pool is open.
pub fn leak_tempdir() -> PathBuf {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();
    std::mem::forget(dir);
    path
}

/// Open a fresh service database inside a leaked temp directory.
pub fn test_storage() -> Arc<Storage> {
    Arc::new(Storage::open(leak_tempdir().join("agentbridge.db")).unwrap())
}

pub fn project_entry(name: &str, path: PathBuf) -> ProjectEntry {
    ProjectEntry {
        id: String::new(),
        name: name.to_string(),
        path,
        description: String::new(),
        readonly: false,
        executor: "opencode".into(),
    }
}

/// Persist the given entries and build a hub backed by the same storage.
pub fn hub_with(
    config: Arc<Config>,
    storage: Arc<Storage>,
    entries: Vec<ProjectEntry>,
) -> ProjectHub {
    for entry in entries {
        storage.upsert_project(entry).unwrap();
    }
    ProjectHub::new(config, storage).unwrap()
}
