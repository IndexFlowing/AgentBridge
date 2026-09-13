//! Runtime ExecutorRegistry hot-reload tests (SQLite-backed Web/API path).

use std::sync::Arc;

use agentbridge::config::{Config, ExecutorDefinition};
use agentbridge::executor::ExecutorError;
use agentbridge::task::PlanInput;
use tempfile::TempDir;

mod common;

/// A custom executor whose command is guaranteed not to exist, so a successful
/// registry lookup surfaces as `NotInstalled` instead of spawning a real process.
fn custom_executor(id: &str) -> ExecutorDefinition {
    ExecutorDefinition {
        id: id.to_string(),
        name: "Custom".into(),
        display_name: "Custom".into(),
        kind: "opencode".into(),
        command: "opencode-not-installed-agentbridge-xyz".into(),
        executable: None,
        working_directory: None,
        proxy_id: None,
        enabled: true,
    }
}

fn sample_plan() -> PlanInput {
    PlanInput {
        actions: vec!["noop".into()],
        tests: vec!["cargo test".into()],
        success_criteria: "noop".into(),
    }
}

fn hub_with_missing_default(
    workspace: &std::path::Path,
) -> (
    Arc<agentbridge::storage::Storage>,
    agentbridge::projects::ProjectHub,
) {
    let mut config = Config::new(workspace.to_path_buf());
    config.executor.command = "opencode-not-installed-agentbridge-xyz".into();
    let storage = common::test_storage();
    let hub = common::hub_with(
        Arc::new(config),
        storage.clone(),
        vec![common::project_entry("default", workspace.to_path_buf())],
    );
    (storage, hub)
}

#[tokio::test(flavor = "multi_thread")]
async fn reload_makes_saved_executor_visible_to_existing_runtime() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("README.md"), "x\n").unwrap();
    let (storage, hub) = hub_with_missing_default(dir.path());

    // Runtime clone held across the reload must observe the updated registry.
    let runtime = (*hub.get("default").unwrap().runtime).clone();

    let err = runtime
        .start_task("goal".into(), sample_plan(), Some("custom-exec"))
        .await
        .unwrap_err();
    assert!(
        matches!(err, ExecutorError::NotFound(_)),
        "before reload: {err}"
    );

    // Equivalent to the Web API POST /executors save flow.
    storage
        .upsert_executor(custom_executor("custom-exec"))
        .unwrap();
    hub.reload_executors().unwrap();

    let err = runtime
        .start_task("goal".into(), sample_plan(), Some("custom-exec"))
        .await
        .unwrap_err();
    assert!(
        matches!(err, ExecutorError::NotInstalled(_)),
        "after reload the executor should resolve: {err}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn reload_after_delete_removes_executor_from_runtime() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("README.md"), "x\n").unwrap();
    let (storage, hub) = hub_with_missing_default(dir.path());

    storage
        .upsert_executor(custom_executor("temp-exec"))
        .unwrap();
    hub.reload_executors().unwrap();

    let runtime = (*hub.get("default").unwrap().runtime).clone();
    let err = runtime
        .start_task("goal".into(), sample_plan(), Some("temp-exec"))
        .await
        .unwrap_err();
    assert!(matches!(err, ExecutorError::NotInstalled(_)), "{err}");

    // Equivalent to the Web API DELETE /executors/{id} flow.
    storage.delete_executor("temp-exec").unwrap();
    hub.reload_executors().unwrap();

    let err = runtime
        .start_task("goal".into(), sample_plan(), Some("temp-exec"))
        .await
        .unwrap_err();
    assert!(
        matches!(err, ExecutorError::NotFound(_)),
        "after delete: {err}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn reload_preserves_default_opencode_executor() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("README.md"), "x\n").unwrap();
    let (_storage, hub) = hub_with_missing_default(dir.path());

    hub.reload_executors().unwrap();

    let runtime = (*hub.get("default").unwrap().runtime).clone();
    let err = runtime
        .start_task("goal".into(), sample_plan(), None)
        .await
        .unwrap_err();
    assert!(
        !matches!(err, ExecutorError::NotFound(_)),
        "default opencode executor must stay registered: {err}"
    );
}
