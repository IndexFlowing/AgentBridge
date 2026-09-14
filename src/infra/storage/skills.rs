// src/infra/storage/skills.rs
//! Storage implementation for Skill metadata and project-level policies.

use rusqlite::{params, OptionalExtension};
use std::path::PathBuf;

use crate::storage::Storage;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredSkillRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub source: String,
    pub path: PathBuf,
    pub enabled: bool,
    pub installed_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProjectSkillPolicyRecord {
    pub project_name: String,
    pub skill_id: String,
    pub enabled: bool,
    pub updated_at: String,
}

impl Storage {
    pub fn load_skills(&self) -> anyhow::Result<Vec<StoredSkillRecord>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, description, version, source, path, enabled, installed_at, updated_at \
             FROM skills ORDER BY name ASC",
        )?;
        let iter = stmt.query_map([], |row| {
            Ok(StoredSkillRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                version: row.get(3)?,
                source: row.get(4)?,
                path: PathBuf::from(row.get::<_, String>(5)?),
                enabled: row.get(6)?,
                installed_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })?;
        let mut out = Vec::new();
        for r in iter {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn get_skill_by_name(&self, name: &str) -> anyhow::Result<Option<StoredSkillRecord>> {
        let conn = self.pool.get()?;
        let skill = conn
            .query_row(
                "SELECT id, name, description, version, source, path, enabled, installed_at, updated_at \
                 FROM skills WHERE name = ?1",
                [name],
                |row| {
                    Ok(StoredSkillRecord {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        description: row.get(2)?,
                        version: row.get(3)?,
                        source: row.get(4)?,
                        path: PathBuf::from(row.get::<_, String>(5)?),
                        enabled: row.get(6)?,
                        installed_at: row.get(7)?,
                        updated_at: row.get(8)?,
                    })
                },
            )
            .optional()?;
        Ok(skill)
    }

    pub fn upsert_skill(&self, skill: StoredSkillRecord) -> anyhow::Result<()> {
        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO skills (id, name, description, version, source, path, enabled, installed_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) \
             ON CONFLICT(id) DO UPDATE SET \
                name = excluded.name, \
                description = excluded.description, \
                version = excluded.version, \
                source = excluded.source, \
                path = excluded.path, \
                enabled = excluded.enabled, \
                updated_at = CURRENT_TIMESTAMP",
            params![
                skill.id,
                skill.name,
                skill.description,
                skill.version,
                skill.source,
                skill.path.to_string_lossy().to_string(),
                skill.enabled
            ],
        )?;
        Ok(())
    }

    pub fn delete_skill(&self, id: &str) -> anyhow::Result<()> {
        let mut conn = self.pool.get()?;
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM project_skills WHERE skill_id = ?1", [id])?;
        tx.execute("DELETE FROM skills WHERE id = ?1", [id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn set_skill_enabled(&self, name: &str, enabled: bool) -> anyhow::Result<()> {
        let conn = self.pool.get()?;
        let rows = conn.execute(
            "UPDATE skills SET enabled = ?1, updated_at = CURRENT_TIMESTAMP WHERE name = ?2",
            params![enabled, name],
        )?;
        if rows == 0 {
            anyhow::bail!("Skill '{name}' not found");
        }
        Ok(())
    }

    pub fn load_project_skill_policies(&self, project_name: &str) -> anyhow::Result<Vec<ProjectSkillPolicyRecord>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT project_name, skill_id, enabled, updated_at FROM project_skills WHERE project_name = ?1",
        )?;
        let iter = stmt.query_map([project_name], |row| {
            Ok(ProjectSkillPolicyRecord {
                project_name: row.get(0)?,
                skill_id: row.get(1)?,
                enabled: row.get(2)?,
                updated_at: row.get(3)?,
            })
        })?;
        let mut out = Vec::new();
        for r in iter {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn set_project_skill_policy(&self, project_name: &str, skill_id: &str, enabled: bool) -> anyhow::Result<()> {
        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO project_skills (project_name, skill_id, enabled, updated_at) \
             VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP) \
             ON CONFLICT(project_name, skill_id) DO UPDATE SET \
                enabled = excluded.enabled, \
                updated_at = CURRENT_TIMESTAMP",
            params![project_name, skill_id, enabled],
        )?;
        Ok(())
    }
}