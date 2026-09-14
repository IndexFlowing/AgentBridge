// src/core/agent.rs
//! Agent Runtime capability.
//!
//! Owns the single entry point for resolving the per-task [`AgentContext`] from
//! the AgentBridge Agent root. Callers (TaskRuntime, SkillService, the Control
//! Plane) depend on this capability instead of reaching into the Skill domain's
//! implementation module, so the resolution policy and its fallback shape live
//! in exactly one place.

use std::path::{Path, PathBuf};

pub use crate::core::skill::agent::{resolve_agent_context, AgentContext, AgentModelError};

/// Resolves a task's [`AgentContext`] against a fixed Agent root.
///
/// This is the owner of the AgentContext resolution capability. It encapsulates
/// which Agent root is used and how a resolution failure degrades to a minimal
/// context, so no caller re-implements those rules.
#[derive(Debug, Clone)]
pub struct AgentContextResolver {
    agent_root: PathBuf,
}

impl AgentContextResolver {
    /// Build a resolver bound to an explicit Agent root.
    pub fn new(agent_root: PathBuf) -> Self {
        Self { agent_root }
    }

    /// Build a resolver bound to the configured AgentBridge Agent root
    /// (`~/.agentbridge/.agent`). Falls back to the same relative layout when no
    /// home directory can be determined, matching historical behavior.
    pub fn from_config() -> Self {
        let root = crate::config::agent_root()
            .unwrap_or_else(|_| PathBuf::from(".agentbridge").join(".agent"));
        Self::new(root)
    }

    /// The Agent root this resolver reads from.
    pub fn agent_root(&self) -> &Path {
        &self.agent_root
    }

    /// Resolve the AgentContext for a task, surfacing resolution errors.
    pub fn resolve(
        &self,
        project_name: &str,
        workspace: &Path,
        task_id: &str,
        requested_skills: &[String],
    ) -> Result<AgentContext, AgentModelError> {
        resolve_agent_context(
            &self.agent_root,
            project_name,
            workspace,
            task_id,
            requested_skills,
        )
    }

    /// Infallible resolution used by the execution path.
    ///
    /// A missing or unreadable Agent root degrades to a minimal context so an
    /// executor can still run; the fallback shape is owned here and never
    /// duplicated by callers.
    pub fn resolve_or_default(
        &self,
        project_name: &str,
        workspace: &Path,
        task_id: &str,
        requested_skills: &[String],
    ) -> AgentContext {
        self.resolve(project_name, workspace, task_id, requested_skills)
            .unwrap_or_else(|err| {
                tracing::warn!(
                    error = %err,
                    agent_root = %self.agent_root.display(),
                    "failed to resolve AgentContext; using minimal context"
                );
                AgentContext {
                    task_id: task_id.to_string(),
                    project: project_name.to_string(),
                    workspace: workspace.display().to_string(),
                    agent_root: self.agent_root.display().to_string(),
                    ..Default::default()
                }
            })
    }
}
