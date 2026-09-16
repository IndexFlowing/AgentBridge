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
///
/// Each entry is `(version, sql)`. The version is recorded in
/// `schema_migrations` so migrations run exactly once even when they contain
/// non-idempotent statements such as `ALTER TABLE ... ADD COLUMN`.
const MIGRATIONS: &[(&str, &str)] = &[
    ("0001_init", include_str!("../../migrations/0001_init.sql")),
    ("0002_skills", include_str!("../../migrations/0002_skills.sql")),
    (
        "0003_task_records",
        include_str!("../../migrations/0003_task_records.sql"),
    ),
    (
        "0004_proxy_metadata",
        include_str!("../../migrations/0004_proxy_metadata.sql"),
    ),
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
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version TEXT PRIMARY KEY,
                applied_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );",
        )?;

        for (version, sql) in MIGRATIONS {
            let applied: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = ?1)",
                [version],
                |row| row.get(0),
            )?;
            if applied {
                continue;
            }
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(sql)?;
            tx.execute(
                "INSERT INTO schema_migrations (version) VALUES (?1)",
                [version],
            )?;
            tx.commit()?;
        }
        Ok(())
    }
}
