use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config;
use crate::protocol::{C2cMessage, C2cState};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TestResult {
    pub status: String,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BridgeState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default)]
    pub iteration: u32,
    pub state: C2cState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changed_files: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tests: Option<TestResult>,
    pub updated_at: DateTime<Utc>,
}

impl Default for BridgeState {
    fn default() -> Self {
        Self {
            task_id: None,
            iteration: 0,
            state: C2cState::Init,
            goal: None,
            status: None,
            changed_files: Vec::new(),
            tests: None,
            updated_at: Utc::now(),
        }
    }
}

impl BridgeState {
    pub fn load(workspace: &Path) -> Result<Self> {
        let path = config::state_path(workspace);
        if !path.is_file() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let state: BridgeState = serde_json::from_str(&text)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(state)
    }

    pub fn save(&self, workspace: &Path) -> Result<()> {
        let dir = config::state_dir(workspace);
        fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
        let path = config::state_path(workspace);
        let tmp = path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(self)?;
        fs::write(&tmp, text)?;
        fs::rename(&tmp, &path).or_else(|_| {
            fs::copy(&tmp, &path).map(|_| ())?;
            fs::remove_file(&tmp)?;
            Ok::<(), anyhow::Error>(())
        })?;
        Ok(())
    }

    pub fn write_c2c(&self, workspace: &Path, extra_notes: Option<&str>) -> Result<()> {
        let msg = self.to_c2c(extra_notes);
        let dir = config::state_dir(workspace);
        fs::create_dir_all(&dir)?;
        fs::write(config::current_c2c_path(workspace), msg.render())?;
        Ok(())
    }

    pub fn to_c2c(&self, extra_notes: Option<&str>) -> C2cMessage {
        let changed = if self.changed_files.is_empty() {
            None
        } else {
            Some(self.changed_files.join("\n"))
        };
        let tests = self.tests.as_ref().map(|t| {
            let mut s = t.command.clone();
            if let Some(code) = t.exit_code {
                s.push_str(&format!("\nexit_code={code}"));
            }
            if let Some(summary) = &t.summary {
                s.push('\n');
                s.push_str(summary);
            }
            s
        });
        C2cMessage {
            state: Some(self.state),
            task_id: self.task_id.clone(),
            iteration: Some(self.iteration),
            goal: self.goal.clone(),
            tests,
            status: self.status.clone(),
            changed_files: changed,
            notes: extra_notes.map(ToOwned::to_owned),
            ..Default::default()
        }
    }

    pub fn execution_summary(&self) -> serde_json::Value {
        serde_json::json!({
            "task_id": self.task_id,
            "iteration": self.iteration,
            "status": self.status,
            "state": self.state.as_str(),
            "changed_files": self.changed_files,
            "tests": self.tests.as_ref().map(|t| serde_json::json!({
                "command": t.command,
                "exit_code": t.exit_code,
                "status": t.status,
                "summary": t.summary,
            })),
            "updated_at": self.updated_at,
        })
    }

    pub fn test_status(&self) -> serde_json::Value {
        match &self.tests {
            Some(t) => serde_json::json!({
                "status": t.status,
                "command": t.command,
                "exit_code": t.exit_code,
                "summary": t.summary,
                "timestamp": t.timestamp,
            }),
            None => serde_json::json!({
                "status": "unknown",
                "message": "No test result has been recorded yet. The Executor should run tests and record them with `agentbridge task executed`."
            }),
        }
    }
}

pub fn new_task_id() -> String {
    let ts = Utc::now().format("%Y%m%d%H%M%S");
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() % 10_000)
        .unwrap_or(0);
    format!("c2c_{ts}_{nanos:04}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn roundtrip_state_file() {
        let dir = TempDir::new().unwrap();
        let state = BridgeState {
            task_id: Some("c2c_1".into()),
            iteration: 2,
            state: C2cState::Executed,
            status: Some("success".into()),
            changed_files: vec!["src/main.rs".into()],
            ..Default::default()
        };
        state.save(dir.path()).unwrap();
        let loaded = BridgeState::load(dir.path()).unwrap();
        assert_eq!(loaded.task_id, state.task_id);
        assert_eq!(loaded.iteration, 2);
        assert_eq!(loaded.state, C2cState::Executed);
        assert_eq!(loaded.changed_files, vec!["src/main.rs".to_string()]);
    }
}
