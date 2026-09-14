// src/storage/executors.rs
use crate::config::ExecutorDefinition;
use crate::storage::Storage;
use std::path::PathBuf;

impl Storage {
    pub fn load_executors(&self) -> anyhow::Result<Vec<ExecutorDefinition>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare("SELECT id, name, kind, command, executable, working_directory, proxy_id, enabled FROM executors")?;

        let iter = stmt.query_map([], |row| {
            Ok(ExecutorDefinition {
                id: row.get(0)?,
                display_name: row.get(1)?,
                name: row.get(1)?,
                kind: row.get(2)?,
                command: row.get(3)?,
                executable: row.get::<_, Option<String>>(4)?.map(PathBuf::from),
                working_directory: row.get::<_, Option<String>>(5)?.map(PathBuf::from),
                proxy_id: row.get(6)?,
                enabled: row.get(7)?,
            })
        })?;

        let mut res = Vec::new();
        for e in iter {
            res.push(e?);
        }
        Ok(res)
    }

    pub fn upsert_executor(&self, mut def: ExecutorDefinition) -> anyhow::Result<()> {
        if def.id.is_empty() {
            def.id = uuid::Uuid::new_v4().to_string();
        }
        self.pool.get()?.execute(
            "INSERT INTO executors (id, name, kind, command, executable, working_directory, proxy_id, enabled) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, kind = excluded.kind, command = excluded.command, executable = excluded.executable, working_directory = excluded.working_directory, proxy_id = excluded.proxy_id, enabled = excluded.enabled",
            rusqlite::params![def.id, def.display_name, def.kind, def.command, def.executable.map(|p| p.to_string_lossy().to_string()), def.working_directory.map(|p| p.to_string_lossy().to_string()), def.proxy_id, def.enabled],
        )?;
        Ok(())
    }

    pub fn delete_executor(&self, id: &str) -> anyhow::Result<()> {
        self.pool
            .get()?
            .execute("DELETE FROM executors WHERE id = ?1", [id])?;
        Ok(())
    }
}
