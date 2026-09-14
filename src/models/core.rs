// src/models/core.rs
//! Core Domain DTOs for Tasks and Projects.
//!
//! Encapsulates request parameters to prevent cascading method signature breakages.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::models::console::ProjectInput;

use crate::protocol::C2cPlan;
use crate::task::PlanInput;

/// Request object to start or iterate a task.
///
/// Designed to be extensible: adding new options (e.g. timeout, skills)
/// will never break downstream method signatures.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StartTaskRequest {
    pub project_name: String,
    pub goal: String,
    pub plan: PlanInput,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub executor: Option<String>,
    #[serde(default)]
    pub continue_task_id: Option<String>,
}

impl StartTaskRequest {
    pub fn new(project_name: impl Into<String>, goal: impl Into<String>, plan: PlanInput) -> Self {
        Self {
            project_name: project_name.into(),
            goal: goal.into(),
            plan,
            skills: Vec::new(),
            executor: None,
            continue_task_id: None,
        }
    }

    pub fn with_skills(mut self, skills: Vec<String>) -> Self {
        self.skills = skills;
        self
    }

    pub fn with_executor(mut self, executor: impl Into<String>) -> Self {
        self.executor = Some(executor.into());
        self
    }

    pub fn with_continue_task_id(mut self, task_id: impl Into<String>) -> Self {
        self.continue_task_id = Some(task_id.into());
        self
    }

    /// Convert this request into a structured C2cPlan with validated identity.
    pub fn into_c2c_plan(
        self,
        task_id: String,
        iteration: u32,
    ) -> Result<C2cPlan, crate::protocol::ProtocolError> {
        C2cPlan::with_skills(
            task_id,
            iteration,
            self.goal,
            self.skills,
            self.plan.actions,
            self.plan.tests,
            self.plan.success_criteria,
        )
    }
}

/// Request object to cancel an ongoing task.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CancelTaskRequest {
    pub project_name: String,
    #[serde(default)]
    pub task_id: Option<String>,
}

/// Request object to create or update a mounted project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveProjectRequest {
    pub id: Option<String>,
    pub name: String,
    pub path: PathBuf,
    pub executor: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub readonly: bool,
}

impl SaveProjectRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("Project name cannot be empty".into());
        }
        if self.executor.trim().is_empty() {
            return Err("Executor cannot be empty".into());
        }
        Ok(())
    }
}

impl From<ProjectInput> for SaveProjectRequest {
    fn from(input: ProjectInput) -> Self {
        Self {
            id: input.id,
            name: input.name,
            path: PathBuf::from(input.path.trim()),
            executor: input.executor,
            description: String::new(),
            readonly: false,
        }
    }
}
