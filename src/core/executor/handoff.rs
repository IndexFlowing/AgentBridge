use std::path::{Path, PathBuf};

use crate::core::agent::AgentContext;
use crate::executor::ExecutorError;
use crate::protocol::C2cPlan;

/// Stage the rendered PLAN + AgentContext into an AgentBridge-managed handoff
/// snapshot and return its absolute path. Shared by every CLI executor so the
/// staging policy lives in exactly one place.
pub fn write_handoff(plan: &C2cPlan, context: &AgentContext) -> Result<PathBuf, ExecutorError> {
    let path = crate::config::handoff_path(&plan.task_id)
        .map_err(|e| ExecutorError::Other(e.to_string()))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ExecutorError::Other(e.to_string()))?;
    }
    let mut content = plan.to_executor_prompt();
    content.push_str(&context.render());
    std::fs::write(&path, content).map_err(|e| ExecutorError::Other(e.to_string()))?;
    Ok(path)
}

/// The single-line instruction handed to the executor CLI.
pub fn executor_prompt(handoff: &Path) -> String {
    format!(
        "Read {} and implement that PLAN. Stay inside this working directory. \
         Run the TESTS. Print a short summary of changed files and test results. \
         Do not paste source or internal reasoning.",
        handoff.display()
    )
}
