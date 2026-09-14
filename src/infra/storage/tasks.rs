// src/storage/tasks.rs
use crate::state::BridgeState;
use crate::storage::Storage;

impl Storage {
    pub fn load_task_state(&self, project_name: &str) -> anyhow::Result<BridgeState> {
        let conn = self.pool.get()?;
        let json_str: String = conn
            .query_row(
                "SELECT state_json FROM tasks WHERE project_name = ?1",
                [project_name],
                |row| row.get(0),
            )
            .unwrap_or_else(|_| "{}".to_string());

        if json_str == "{}" {
            return Ok(BridgeState::default());
        }

        let state = serde_json::from_str(&json_str)?;
        Ok(state)
    }

    pub fn save_task_state(&self, project_name: &str, state: &BridgeState) -> anyhow::Result<()> {
        let json_str = serde_json::to_string(state)?;
        self.pool.get()?.execute(
            "INSERT INTO tasks (project_name, state_json) VALUES (?1, ?2) ON CONFLICT(project_name) DO UPDATE SET state_json = excluded.state_json, updated_at = CURRENT_TIMESTAMP",
            rusqlite::params![project_name, json_str],
        )?;
        Ok(())
    }
}
