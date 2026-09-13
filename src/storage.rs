// src/storage.rs
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::credentials::CredentialCipher;

pub mod executors;
pub mod oauth;
pub mod projects;
pub mod providers;
pub mod proxies;
pub mod tasks;

pub type DbPool = Pool<SqliteConnectionManager>;

#[derive(Clone)]
pub struct Storage {
    pub pool: DbPool,
    /// Local protection key for Provider credentials. Never a Provider API key.
    pub credentials: Arc<CredentialCipher>,
}

impl Storage {
    /// Open (creating if needed) the service database at an explicit path.
    /// Used by the CLI defaults and by tests; callers own the location.
    pub fn open(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let cipher = CredentialCipher::load_for(path.as_ref())?;
        Self::open_with_cipher(path, cipher)
    }

    /// Open with an explicit credential protection cipher.
    ///
    /// Useful for tests and embedded callers that manage the key themselves.
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
            r#"
            CREATE TABLE IF NOT EXISTS projects (id TEXT PRIMARY KEY, name TEXT NOT NULL, path TEXT NOT NULL, description TEXT NOT NULL DEFAULT '', readonly BOOLEAN NOT NULL DEFAULT 0, executor TEXT NOT NULL, active BOOLEAN NOT NULL DEFAULT 0, created_at DATETIME DEFAULT CURRENT_TIMESTAMP);
            CREATE TABLE IF NOT EXISTS executors (id TEXT PRIMARY KEY, name TEXT NOT NULL, kind TEXT NOT NULL, command TEXT NOT NULL, executable TEXT, working_directory TEXT, proxy_id TEXT, enabled BOOLEAN NOT NULL DEFAULT 1);
            
            CREATE TABLE IF NOT EXISTS oauth_clients (client_id TEXT PRIMARY KEY, client_secret TEXT, client_name TEXT NOT NULL, redirect_uris TEXT NOT NULL, auth_method TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS oauth_pending (request_id TEXT PRIMARY KEY, client_id TEXT NOT NULL, redirect_uri TEXT NOT NULL, state TEXT, code_challenge TEXT NOT NULL, scope TEXT NOT NULL, resource TEXT, expires_at INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS oauth_codes (code TEXT PRIMARY KEY, client_id TEXT NOT NULL, redirect_uri TEXT NOT NULL, code_challenge TEXT NOT NULL, scope TEXT NOT NULL, resource TEXT, expires_at INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS oauth_access (token TEXT PRIMARY KEY, client_id TEXT NOT NULL, scope TEXT NOT NULL, expires_at INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS oauth_refresh (token TEXT PRIMARY KEY, client_id TEXT NOT NULL, scope TEXT NOT NULL, expires_at INTEGER NOT NULL);
            
            -- 新增：将原先的 state.json 存储为数据库记录
            CREATE TABLE IF NOT EXISTS tasks (
                project_name TEXT PRIMARY KEY,
                state_json TEXT NOT NULL,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS proxies (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                host TEXT NOT NULL,
                port INTEGER NOT NULL,
                username TEXT,
                password TEXT,
                enabled BOOLEAN NOT NULL DEFAULT 1,
                is_default BOOLEAN NOT NULL DEFAULT 0
            );

            -- Provider Core: SQLite is the source of truth for Providers/Models.
            CREATE TABLE IF NOT EXISTS providers (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                base_url TEXT NOT NULL DEFAULT '',
                enabled BOOLEAN NOT NULL DEFAULT 1,
                is_default BOOLEAN NOT NULL DEFAULT 0,
                proxy_id TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS provider_models (
                id TEXT PRIMARY KEY,
                provider_id TEXT NOT NULL,
                name TEXT NOT NULL,
                enabled BOOLEAN NOT NULL DEFAULT 1,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS provider_credentials (
                provider_id TEXT PRIMARY KEY,
                secret TEXT NOT NULL,
                scheme TEXT NOT NULL DEFAULT 'aes-256-gcm',
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_provider_models_provider
                ON provider_models(provider_id);
            CREATE UNIQUE INDEX IF NOT EXISTS idx_providers_single_default
                ON providers(is_default) WHERE is_default = 1;
            "#,
        )?;
        Ok(())
    }
}
