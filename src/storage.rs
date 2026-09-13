// src/storage.rs
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
use std::path::{Path, PathBuf};

pub mod executors;
pub mod oauth;
pub mod projects;
pub mod tasks; // <--- 新增 tasks 模块

pub type DbPool = Pool<SqliteConnectionManager>;

#[derive(Clone)]
pub struct Storage {
    pub pool: DbPool,
}

impl Storage {
    /// Open (creating if needed) the service database at an explicit path.
    /// Used by the CLI defaults and by tests; callers own the location.
    pub fn open(path: impl AsRef<Path>) -> anyhow::Result<Self> {
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

        Ok(Self { pool })
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
            "#,
        )?;
        Ok(())
    }
}
