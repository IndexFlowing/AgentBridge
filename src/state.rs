use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config;
use crate::protocol::{C2cMessage, C2cPlan, C2cState};

/// Operational lifecycle of a Task. Distinct from C2C STATE.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Created,
    Planned,
    Running,
    Executed,
    Review,
    Done,
    Failed,
    Blocked,
    Cancelled,
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Planned => "planned",
            Self::Running => "running",
            Self::Executed => "executed",
            Self::Review => "review",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Blocked => "blocked",
            Self::Cancelled => "cancelled",
        }
    }

    /// Status string returned by the MCP `task_status` tool.
    pub fn result_status(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Failed => "failed",
            Self::Blocked => "blocked",
            Self::Cancelled => "cancelled",
            Self::Created | Self::Planned => "planned",
            Self::Executed | Self::Review | Self::Done => "success",
        }
    }

    pub fn is_running(self) -> bool {
        matches!(self, Self::Running)
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Executed
                | Self::Review
                | Self::Done
                | Self::Failed
                | Self::Blocked
                | Self::Cancelled
        )
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

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
    pub task_status: Option<TaskStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success_criteria: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executor: Option<String>,
    /// Result status: running | success | failed | blocked | cancelled
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
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
            task_status: None,
            goal: None,
            actions: Vec::new(),
            success_criteria: None,
            workspace: None,
            executor: None,
            status: None,
            created_at: None,
            started_at: None,
            finished_at: None,
            exit_code: None,
            summary: None,
            error: None,
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
        let actions = if self.actions.is_empty() {
            None
        } else {
            Some(self.actions.join("\n"))
        };
        C2cMessage {
            state: Some(self.state),
            task_id: self.task_id.clone(),
            iteration: Some(self.iteration),
            goal: self.goal.clone(),
            actions,
            tests,
            success_criteria: self.success_criteria.clone(),
            status: self.status.clone(),
            changed_files: changed,
            result: self.summary.clone(),
            notes: extra_notes.map(ToOwned::to_owned),
            ..Default::default()
        }
    }

    pub fn apply_plan(&mut self, plan: &C2cPlan, workspace: &str, executor: &str) {
        let now = Utc::now();
        if self.created_at.is_none() || self.task_id.as_deref() != Some(plan.task_id.as_str()) {
            self.created_at = Some(now);
        }
        self.task_id = Some(plan.task_id.clone());
        self.iteration = plan.iteration;
        self.state = C2cState::Plan;
        self.task_status = Some(TaskStatus::Planned);
        self.goal = Some(plan.goal.clone());
        self.actions = plan.actions.clone();
        self.success_criteria = Some(plan.success_criteria.clone());
        self.workspace = Some(workspace.to_string());
        self.executor = Some(executor.to_string());
        self.status = Some("planned".into());
        self.started_at = None;
        self.finished_at = None;
        self.exit_code = None;
        self.summary = None;
        self.error = None;
        self.changed_files.clear();
        self.tests = plan.tests_command().map(|command| TestResult {
            status: "pending".into(),
            command,
            exit_code: None,
            summary: None,
            timestamp: now,
        });
        self.updated_at = now;
    }

    pub fn mark_running(&mut self) {
        let now = Utc::now();
        self.state = C2cState::Executing;
        self.task_status = Some(TaskStatus::Running);
        self.status = Some("running".into());
        self.started_at = Some(now);
        self.finished_at = None;
        self.error = None;
        self.updated_at = now;
    }

    pub fn execution_summary(&self) -> serde_json::Value {
        serde_json::json!({
            "task_id": self.task_id,
            "iteration": self.iteration,
            "status": self.status,
            "state": self.state.as_str(),
            "lifecycle": self.task_status.map(|s| s.as_str()),
            "executor": self.executor,
            "exit_code": self.exit_code,
            "summary": self.summary,
            "error": self.error,
            "changed_files": self.changed_files,
            "tests": self.tests.as_ref().map(|t| serde_json::json!({
                "command": t.command,
                "exit_code": t.exit_code,
                "status": t.status,
                "summary": t.summary,
            })),
            "created_at": self.created_at,
            "started_at": self.started_at,
            "finished_at": self.finished_at,
            "updated_at": self.updated_at,
        })
    }

    pub fn task_status_payload(&self) -> serde_json::Value {
        let status = self
            .task_status
            .map(|s| s.result_status())
            .or(self.status.as_deref())
            .unwrap_or("planned");
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
        serde_json::json!({
            "task_id": self.task_id,
            "iteration": self.iteration,
            "lifecycle": self.task_status.map(|s| s.as_str()),
            "status": status,
            "summary": self.summary,
            "exit_code": self.exit_code,
            "tests": tests,
            "changed_files": self.changed_files,
            "error": self.error,
            "executor": self.executor,
            "started_at": self.started_at,
            "finished_at": self.finished_at,
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
                "message": "No test result has been recorded yet. The Executor runs tests; this tool only reports the last recorded result."
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
