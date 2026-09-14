// src/infra/storage.rs
//! SQLite Storage Layer with compile-time embedded SQL migrations.

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::provider::credentials::CredentialCipher;

pub mod executors;
pub mod oauth;
pub mod projects;
pub mod providers;
pub mod proxies;
pub mod skills;
pub mod tasks;

pub type DbPool = Pool<SqliteConnectionManager>;

/// Ordered schema migrations embedded into the binary at compile time.
const MIGRATIONS: &[&str] = &[
    include_str!("../../migrations/0001_init.sql"),
    include_str!("../../migrations/0002_skills.sql"),
    include_str!("../../migrations/0003_task_records.sql"),
];

#[derive(Clone)]
pub struct Storage {
    // Kept private: upper layers must go through typed repository methods or an
    // Application Service rather than the raw connection pool.
    pool: DbPool,
    credentials: Arc<CredentialCipher>,
}

impl Storage {
    pub fn open(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let cipher = CredentialCipher::load_for(path.as_ref())?;
        Self::open_with_cipher(path, cipher)
    }

    pub fn open_with_cipher(
        path: impl AsRef<Path>,
        cipher: CredentialCipher,
    ) -> anyhow::Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let manager = SqliteConnectionManager::file(path);
        let pool = Pool::new(manager)?;
        let conn = pool.get()?;
        Self::run_migrations(&conn)?;

        Ok(Self {
            pool,
            credentials: Arc::new(cipher),
        })
    }

    pub fn init() -> anyhow::Result<Self> {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        Self::open(home.join(".agentbridge").join("agentbridge.db"))
    }

    fn run_migrations(conn: &Connection) -> anyhow::Result<()> {
        for sql in MIGRATIONS {
            conn.execute_batch(sql)?;
        }
        Ok(())
    }
}
