// src/storage/providers.rs
use rusqlite::{params, OptionalExtension};

use crate::provider::{CredentialMetadata, ModelDefinition, ProviderDefinition};
use crate::storage::Storage;

impl Storage {
    pub fn load_providers(&self) -> anyhow::Result<Vec<ProviderDefinition>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, kind, base_url, enabled, is_default, proxy_id \
             FROM providers ORDER BY name ASC",
        )?;
        let iter = stmt.query_map([], |row| {
            Ok(ProviderDefinition {
                id: row.get(0)?,
                name: row.get(1)?,
                kind: row.get(2)?,
                base_url: row.get(3)?,
                enabled: row.get(4)?,
                is_default: row.get(5)?,
                proxy_id: row.get(6)?,
            })
        })?;
        let mut providers = Vec::new();
        for provider in iter {
            providers.push(provider?);
        }
        Ok(providers)
    }

    pub fn get_provider(&self, id: &str) -> anyhow::Result<Option<ProviderDefinition>> {
        let conn = self.pool.get()?;
        let provider = conn
            .query_row(
                "SELECT id, name, kind, base_url, enabled, is_default, proxy_id FROM providers WHERE id = ?1",
                [id],
                |row| {
                    Ok(ProviderDefinition {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        kind: row.get(2)?,
                        base_url: row.get(3)?,
                        enabled: row.get(4)?,
                        is_default: row.get(5)?,
                        proxy_id: row.get(6)?,
                    })
                },
            )
            .optional()?;
        Ok(provider)
    }

    /// Insert or update a Provider, enforcing a single default Provider.
    pub fn upsert_provider(&self, mut def: ProviderDefinition) -> anyhow::Result<()> {
        if def.id.trim().is_empty() {
            def.id = uuid::Uuid::new_v4().to_string();
        }
        if def.name.trim().is_empty() {
            anyhow::bail!("provider name is required");
        }
        if def.kind.trim().is_empty() {
            anyhow::bail!("provider type is required");
        }

        let mut conn = self.pool.get()?;
        let tx = conn.transaction()?;
        if def.is_default {
            tx.execute(
                "UPDATE providers SET is_default = 0 WHERE is_default = 1",
                [],
            )?;
        }
        tx.execute(
            "INSERT INTO providers (id, name, kind, base_url, enabled, is_default, proxy_id, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, CURRENT_TIMESTAMP)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                kind = excluded.kind,
                base_url = excluded.base_url,
                enabled = excluded.enabled,
                is_default = excluded.is_default,
                proxy_id = excluded.proxy_id,
                updated_at = CURRENT_TIMESTAMP",
            params![
                def.id,
                def.name,
                def.kind,
                def.base_url,
                def.enabled,
                def.is_default,
                def.proxy_id,
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn delete_provider(&self, id: &str) -> anyhow::Result<()> {
        let mut conn = self.pool.get()?;
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM provider_models WHERE provider_id = ?1", [id])?;
        tx.execute(
            "DELETE FROM provider_credentials WHERE provider_id = ?1",
            [id],
        )?;
        tx.execute("DELETE FROM providers WHERE id = ?1", [id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn load_models(&self) -> anyhow::Result<Vec<ModelDefinition>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, provider_id, name, enabled FROM provider_models ORDER BY name ASC",
        )?;
        let iter = stmt.query_map([], |row| {
            Ok(ModelDefinition {
                id: row.get(0)?,
                provider_id: row.get(1)?,
                name: row.get(2)?,
                enabled: row.get(3)?,
            })
        })?;
        let mut models = Vec::new();
        for model in iter {
            models.push(model?);
        }
        Ok(models)
    }

    pub fn load_models_for_provider(
        &self,
        provider_id: &str,
    ) -> anyhow::Result<Vec<ModelDefinition>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, provider_id, name, enabled FROM provider_models \
             WHERE provider_id = ?1 ORDER BY name ASC",
        )?;
        let iter = stmt.query_map([provider_id], |row| {
            Ok(ModelDefinition {
                id: row.get(0)?,
                provider_id: row.get(1)?,
                name: row.get(2)?,
                enabled: row.get(3)?,
            })
        })?;
        let mut models = Vec::new();
        for model in iter {
            models.push(model?);
        }
        Ok(models)
    }

    pub fn upsert_model(&self, mut def: ModelDefinition) -> anyhow::Result<()> {
        if def.id.trim().is_empty() {
            def.id = uuid::Uuid::new_v4().to_string();
        }
        if def.provider_id.trim().is_empty() {
            anyhow::bail!("model provider is required");
        }
        if def.name.trim().is_empty() {
            anyhow::bail!("model name is required");
        }
        let conn = self.pool.get()?;
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM providers WHERE id = ?1",
                [&def.provider_id],
                |_| Ok(true),
            )
            .optional()?
            .is_some();
        if !exists {
            anyhow::bail!("provider not found: {}", def.provider_id);
        }
        conn.execute(
            "INSERT INTO provider_models (id, provider_id, name, enabled)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET
                provider_id = excluded.provider_id,
                name = excluded.name,
                enabled = excluded.enabled",
            params![def.id, def.provider_id, def.name, def.enabled],
        )?;
        Ok(())
    }

    pub fn delete_model(&self, id: &str) -> anyhow::Result<()> {
        self.pool
            .get()?
            .execute("DELETE FROM provider_models WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Encrypt and persist a Provider secret, preserving other fields.
    pub fn save_provider_secret(&self, provider_id: &str, secret: &str) -> anyhow::Result<()> {
        if secret.trim().is_empty() {
            anyhow::bail!("credential secret must not be empty");
        }
        let conn = self.pool.get()?;
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM providers WHERE id = ?1",
                [provider_id],
                |_| Ok(true),
            )
            .optional()?
            .is_some();
        if !exists {
            anyhow::bail!("provider not found: {provider_id}");
        }
        let sealed = self.credentials.encrypt(secret)?;
        conn.execute(
            "INSERT INTO provider_credentials (provider_id, secret, scheme, updated_at)
             VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP)
             ON CONFLICT(provider_id) DO UPDATE SET
                secret = excluded.secret,
                scheme = excluded.scheme,
                updated_at = CURRENT_TIMESTAMP",
            params![provider_id, sealed, self.credentials.scheme()],
        )?;
        Ok(())
    }

    /// Non-secret metadata about a Provider credential.
    pub fn provider_credential_metadata(
        &self,
        provider_id: &str,
    ) -> anyhow::Result<Option<CredentialMetadata>> {
        let conn = self.pool.get()?;
        let updated_at = conn
            .query_row(
                "SELECT updated_at FROM provider_credentials WHERE provider_id = ?1",
                [provider_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?;
        Ok(updated_at.map(|updated_at| CredentialMetadata {
            provider_id: provider_id.to_string(),
            configured: true,
            updated_at,
        }))
    }

    pub fn provider_has_secret(&self, provider_id: &str) -> bool {
        self.provider_credential_metadata(provider_id)
            .ok()
            .flatten()
            .is_some()
    }

    /// Decrypt a Provider secret. Callers must never log or serialize the result.
    pub fn get_provider_secret(&self, provider_id: &str) -> anyhow::Result<Option<String>> {
        let conn = self.pool.get()?;
        let sealed = conn
            .query_row(
                "SELECT secret FROM provider_credentials WHERE provider_id = ?1",
                [provider_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        match sealed {
            Some(sealed) => Ok(Some(self.credentials.decrypt(&sealed)?)),
            None => Ok(None),
        }
    }

    pub fn delete_provider_secret(&self, provider_id: &str) -> anyhow::Result<()> {
        self.pool.get()?.execute(
            "DELETE FROM provider_credentials WHERE provider_id = ?1",
            [provider_id],
        )?;
        Ok(())
    }
}
