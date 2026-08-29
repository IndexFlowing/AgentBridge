//! Executor adapters are intentionally tiny in V0.1.
//!
//! AgentBridge does not drive a coding agent. The Executor (OpenCode, Codex,
//! Claude Code, or a human) edits the workspace and records the result with:
//!
//! ```text
//! agentbridge task executed --task-id ... --status success --tests "cargo test" --exit-code 0
//! ```
//!
//! Future versions can implement this trait for native adapters.

/// Local coding agent that is allowed to write files and run commands.
pub trait ExecutorAdapter: Send + Sync {
    fn name(&self) -> &'static str;
}

/// Documented placeholder. Results are recorded through the CLI.
pub struct ExternalExecutor {
    name: &'static str,
}

impl ExternalExecutor {
    pub fn new(name: &'static str) -> Self {
        Self { name }
    }
}

impl ExecutorAdapter for ExternalExecutor {
    fn name(&self) -> &'static str {
        self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_name() {
        let exec = ExternalExecutor::new("opencode");
        assert_eq!(exec.name(), "opencode");
    }
}
