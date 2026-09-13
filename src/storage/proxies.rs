// src/storage/proxies.rs
use crate::config::{ProxyConfig, ProxyKind};
use crate::storage::Storage;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProxyDefinition {
    pub id: String,
    pub name: String,
    pub kind: ProxyKind,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub enabled: bool,
    pub is_default: bool,
}

impl ProxyDefinition {
    pub fn to_config(&self) -> ProxyConfig {
        ProxyConfig {
            enabled: self.enabled,
            kind: self.kind,
            host: self.host.clone(),
            port: self.port,
            username: self.username.clone(),
            password: self.password.clone(),
        }
    }
}

impl Storage {
    pub fn load_proxies(&self) -> anyhow::Result<Vec<ProxyDefinition>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, kind, host, port, username, password, enabled, is_default FROM proxies",
        )?;

        let iter = stmt.query_map([], |row| {
            let kind_str: String = row.get(2)?;
            Ok(ProxyDefinition {
                id: row.get(0)?,
                name: row.get(1)?,
                kind: ProxyKind::from_str_opt(&kind_str),
                host: row.get(3)?,
                port: row.get(4)?,
                username: row.get(5)?,
                password: row.get(6)?,
                enabled: row.get(7)?,
                is_default: row.get(8)?,
            })
        })?;

        let mut res = Vec::new();
        for p in iter {
            res.push(p?);
        }
        Ok(res)
    }

    pub fn upsert_proxy(&self, mut def: ProxyDefinition) -> anyhow::Result<()> {
        if def.id.trim().is_empty() {
            def.id = uuid::Uuid::new_v4().to_string();
        }
        let conn = self.pool.get()?;
        if def.is_default {
            // 若设为默认，先清空其他默认
            let _ = conn.execute("UPDATE proxies SET is_default = 0", []);
        }
        conn.execute(
            "INSERT INTO proxies (id, name, kind, host, port, username, password, enabled, is_default)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                kind = excluded.kind,
                host = excluded.host,
                port = excluded.port,
                username = excluded.username,
                password = excluded.password,
                enabled = excluded.enabled,
                is_default = excluded.is_default",
            rusqlite::params![
                def.id,
                def.name,
                def.kind.as_str(),
                def.host,
                def.port,
                def.username,
                def.password,
                def.enabled,
                def.is_default
            ],
        )?;
        Ok(())
    }

    pub fn delete_proxy(&self, id: &str) -> anyhow::Result<()> {
        self.pool
            .get()?
            .execute("DELETE FROM proxies WHERE id = ?1", [id])?;
        Ok(())
    }
}