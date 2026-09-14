// src/storage/projects.rs
use crate::projects::ProjectEntry;
use crate::storage::Storage;
use std::path::PathBuf;

impl Storage {
    pub fn load_projects(&self) -> anyhow::Result<Vec<ProjectEntry>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare("SELECT id, name, path, description, readonly, executor FROM projects ORDER BY created_at ASC")?;

        let iter = stmt.query_map([], |row| {
            Ok(ProjectEntry {
                id: row.get(0)?,
                name: row.get(1)?,
                path: PathBuf::from(row.get::<_, String>(2)?),
                description: row.get(3)?,
                readonly: row.get(4)?,
                executor: row.get(5)?,
            })
        })?;

        let mut res = Vec::new();
        for p in iter {
            res.push(p?);
        }
        Ok(res)
    }

    pub fn upsert_project(&self, mut entry: ProjectEntry) -> anyhow::Result<()> {
        if entry.id.is_empty() {
            entry.id = uuid::Uuid::new_v4().to_string();
        }
        self.pool.get()?.execute(
            "INSERT INTO projects (id, name, path, description, readonly, executor) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, path = excluded.path, description = excluded.description, readonly = excluded.readonly, executor = excluded.executor",
            rusqlite::params![entry.id, entry.name, entry.path.to_string_lossy().to_string(), entry.description, entry.readonly, entry.executor],
        )?;
        Ok(())
    }

    pub fn delete_project(&self, id: &str) -> anyhow::Result<()> {
        self.pool
            .get()?
            .execute("DELETE FROM projects WHERE id = ?1", [id])?;
        Ok(())
    }
}
