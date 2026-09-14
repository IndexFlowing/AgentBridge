// src/storage/oauth.rs
use crate::oauth::server::ConnectedClientInfo;
use crate::oauth::storage::{AuthCode, IssuedToken, PendingAuth, RegisteredClient};
use crate::storage::Storage;

impl Storage {
    pub fn get_oauth_client(&self, client_id: &str) -> Option<RegisteredClient> {
        self.pool.get().ok()?.query_row(
            "SELECT client_id, client_secret, client_name, redirect_uris, auth_method FROM oauth_clients WHERE client_id = ?1",
            [client_id],
            |row| {
                let uris_str: String = row.get(3)?;
                Ok(RegisteredClient {
                    client_id: row.get(0)?,
                    client_secret: row.get(1)?,
                    client_name: row.get(2)?,
                    redirect_uris: serde_json::from_str(&uris_str).unwrap_or_default(),
                    token_endpoint_auth_method: row.get(4)?,
                })
            },
        ).ok()
    }

    pub fn insert_oauth_client(&self, client: &RegisteredClient) -> anyhow::Result<()> {
        let uris = serde_json::to_string(&client.redirect_uris).unwrap_or_else(|_| "[]".into());
        self.pool.get()?.execute(
            "INSERT INTO oauth_clients (client_id, client_secret, client_name, redirect_uris, auth_method) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![client.client_id, client.client_secret, client.client_name, uris, client.token_endpoint_auth_method],
        )?;
        Ok(())
    }

    pub fn insert_oauth_pending(&self, request_id: &str, p: &PendingAuth) -> anyhow::Result<()> {
        self.pool.get()?.execute(
            "INSERT INTO oauth_pending (request_id, client_id, redirect_uri, state, code_challenge, scope, resource, expires_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![request_id, p.client_id, p.redirect_uri, p.state, p.code_challenge, p.scope, p.resource, p.expires_at],
        )?;
        Ok(())
    }

    pub fn take_oauth_pending(&self, request_id: &str) -> Option<PendingAuth> {
        let conn = self.pool.get().ok()?;
        let p = conn.query_row("SELECT client_id, redirect_uri, state, code_challenge, scope, resource, expires_at FROM oauth_pending WHERE request_id = ?1", [request_id], |row| {
            Ok(PendingAuth {
                client_id: row.get(0)?, redirect_uri: row.get(1)?, state: row.get(2)?,
                code_challenge: row.get(3)?, scope: row.get(4)?, resource: row.get(5)?, expires_at: row.get(6)?,
            })
        }).ok()?;
        let _ = conn.execute(
            "DELETE FROM oauth_pending WHERE request_id = ?1",
            [request_id],
        );
        Some(p)
    }

    pub fn insert_oauth_code(&self, code: &str, c: &AuthCode) -> anyhow::Result<()> {
        self.pool.get()?.execute(
            "INSERT INTO oauth_codes (code, client_id, redirect_uri, code_challenge, scope, resource, expires_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![code, c.client_id, c.redirect_uri, c.code_challenge, c.scope, c.resource, c.expires_at],
        )?;
        Ok(())
    }

    pub fn take_oauth_code(&self, code: &str) -> Option<AuthCode> {
        let conn = self.pool.get().ok()?;
        let c = conn.query_row("SELECT client_id, redirect_uri, code_challenge, scope, resource, expires_at FROM oauth_codes WHERE code = ?1", [code], |row| {
            Ok(AuthCode {
                client_id: row.get(0)?, redirect_uri: row.get(1)?, code_challenge: row.get(2)?,
                scope: row.get(3)?, resource: row.get(4)?, expires_at: row.get(5)?,
            })
        }).ok()?;
        let _ = conn.execute("DELETE FROM oauth_codes WHERE code = ?1", [code]);
        Some(c)
    }

    pub fn issue_tokens(
        &self,
        access: &str,
        refresh: &str,
        t: &IssuedToken,
        refresh_expires: u64,
    ) -> anyhow::Result<()> {
        let conn = self.pool.get()?;
        conn.execute("INSERT INTO oauth_access (token, client_id, scope, expires_at) VALUES (?1, ?2, ?3, ?4)", rusqlite::params![access, t.client_id, t.scope, t.expires_at])?;
        conn.execute("INSERT INTO oauth_refresh (token, client_id, scope, expires_at) VALUES (?1, ?2, ?3, ?4)", rusqlite::params![refresh, t.client_id, t.scope, refresh_expires])?;
        Ok(())
    }

    pub fn is_access_token_valid(&self, token: &str, now: u64) -> bool {
        self.pool
            .get()
            .ok()
            .and_then(|c| {
                c.query_row(
                    "SELECT 1 FROM oauth_access WHERE token = ?1 AND expires_at > ?2",
                    rusqlite::params![token, now],
                    |_| Ok(()),
                )
                .ok()
            })
            .is_some()
    }

    pub fn take_refresh_token(&self, token: &str) -> Option<IssuedToken> {
        let conn = self.pool.get().ok()?;
        let t = conn
            .query_row(
                "SELECT client_id, scope, expires_at FROM oauth_refresh WHERE token = ?1",
                [token],
                |row| {
                    Ok(IssuedToken {
                        client_id: row.get(0)?,
                        scope: row.get(1)?,
                        expires_at: row.get(2)?,
                    })
                },
            )
            .ok()?;
        let _ = conn.execute("DELETE FROM oauth_refresh WHERE token = ?1", [token]);
        Some(t)
    }

    pub fn revoke_token(&self, token: &str) {
        if let Ok(conn) = self.pool.get() {
            let _ = conn.execute("DELETE FROM oauth_access WHERE token = ?1", [token]);
            let _ = conn.execute("DELETE FROM oauth_refresh WHERE token = ?1", [token]);
        }
    }

    pub fn get_active_clients(&self, now: u64) -> Vec<ConnectedClientInfo> {
        let conn = match self.pool.get() {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        let mut stmt = conn.prepare("SELECT DISTINCT c.client_id, c.client_name FROM oauth_access a JOIN oauth_clients c ON a.client_id = c.client_id WHERE a.expires_at > ?1").unwrap();
        let iter = stmt
            .query_map([now], |row| {
                Ok(ConnectedClientInfo {
                    client_id: row.get(0)?,
                    client_name: row.get(1)?,
                    has_active_token: true,
                })
            })
            .unwrap();
        iter.filter_map(|r| r.ok()).collect()
    }
}
