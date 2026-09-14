// src/storage/tasks.rs
use rusqlite::OptionalExtension;

use crate::state::BridgeState;
use crate::storage::Storage;

impl Storage {
    /// Latest persisted task for a project.
    ///
    /// This is an explicit project-dimension convenience (dashboard/list default)
    /// and is never used to resolve a concrete `task_id`.
    pub fn load_task_state(&self, project_name: &str) -> anyhow::Result<BridgeState> {
        let conn = self.pool.get()?;
        let json_str: Option<String> = conn
            .query_row(
                "SELECT state_json FROM task_records WHERE project_name = ?1 \
                 ORDER BY updated_at DESC, rowid DESC LIMIT 1",
                [project_name],
                |row| row.get(0),
            )
            .optional()?;

        if let Some(json) = json_str {
            return Ok(serde_json::from_str(&json)?);
        }

        // Legacy fallback for pre-migration rows that carry no task_id.
        let legacy: Option<String> = conn
            .query_row(
                "SELECT state_json FROM tasks WHERE project_name = ?1",
                [project_name],
                |row| row.get(0),
            )
            .optional()?;
        match legacy {
            Some(json) if json != "{}" => Ok(serde_json::from_str(&json)?),
            _ => Ok(BridgeState::default()),
        }
    }

    /// Exact lookup by the stable task identity. Never falls back to a project.
    pub fn load_task_state_by_id(&self, task_id: &str) -> anyhow::Result<Option<BridgeState>> {
        let conn = self.pool.get()?;
        let json: Option<String> = conn
            .query_row(
                "SELECT state_json FROM task_records WHERE task_id = ?1",
                [task_id],
                |row| row.get(0),
            )
            .optional()?;
        json.map(|j| serde_json::from_str::<BridgeState>(&j).map_err(Into::into))
            .transpose()
    }

    /// Persist a task under its `task_id`.
    ///
    /// Rows are keyed by task identity, so saving one task can never overwrite a
    /// sibling task inside the same project. Legacy anonymous (id-less) writes
    /// keep using the old project-keyed table.
    pub fn save_task_state(&self, project_name: &str, state: &BridgeState) -> anyhow::Result<()> {
        let json_str = serde_json::to_string(state)?;
        let conn = self.pool.get()?;

        let task_id = state
            .task_id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty());

        match task_id {
            Some(task_id) => {
                conn.execute(
                    "INSERT INTO task_records (task_id, project_name, state_json, updated_at) \
                     VALUES (?1, ?2, ?3, strftime('%Y-%m-%dT%H:%M:%fZ','now')) \
                     ON CONFLICT(task_id) DO UPDATE SET \
                        project_name = excluded.project_name, \
                        state_json = excluded.state_json, \
                        updated_at = excluded.updated_at",
                    rusqlite::params![task_id, project_name, json_str],
                )?;
            }
            None => {
                conn.execute(
                    "INSERT INTO tasks (project_name, state_json) VALUES (?1, ?2) \
                     ON CONFLICT(project_name) DO UPDATE SET \
                        state_json = excluded.state_json, updated_at = CURRENT_TIMESTAMP",
                    rusqlite::params![project_name, json_str],
                )?;
            }
        }
        Ok(())
    }

    /// Locate the project that owns `task_id` and return its persisted state.
    ///
    /// Task identity is the globally unique `task_id`; the per-project storage
    /// row is only a physical container.
    pub fn find_task_by_id(&self, task_id: &str) -> anyhow::Result<Option<(String, BridgeState)>> {
        let conn = self.pool.get()?;
        let row: Option<(String, String)> = conn
            .query_row(
                "SELECT project_name, state_json FROM task_records WHERE task_id = ?1",
                [task_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        match row {
            Some((project, json)) => Ok(Some((project, serde_json::from_str(&json)?))),
            None => Ok(None),
        }
    }

    /// All persisted tasks for a project, newest first.
    pub fn list_task_states(&self, project_name: &str) -> anyhow::Result<Vec<BridgeState>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT state_json FROM task_records WHERE project_name = ?1 \
             ORDER BY updated_at DESC, rowid DESC",
        )?;
        let rows = stmt.query_map([project_name], |row| row.get::<_, String>(0))?;
        let mut states = Vec::new();
        for row in rows {
            states.push(serde_json::from_str(&row?)?);
        }
        Ok(states)
    }
}
