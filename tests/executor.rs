//! OpenCode executor integration tests.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

use agentbridge::config::Config;
use agentbridge::doctor::{self, CheckStatus};
use agentbridge::executor::{self, ExecutorError};
use agentbridge::git;
use agentbridge::models::{CancelTaskRequest, StartTaskRequest};
use agentbridge::state::TaskStatus;
use agentbridge::task::{PlanInput, TaskRuntime, TaskService};

mod common;

#[derive(Clone, Copy)]
enum FakeKind {
    Success,
    Fail,
    Hang,
}

fn git_workspace() -> TempDir {
    let dir = TempDir::new().unwrap();
    let run = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .env("GIT_AUTHOR_NAME", "AgentBridge")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "AgentBridge")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .status()
            .unwrap();
    };
    run(&["init", "-b", "main"]);
    run(&["config", "user.name", "AgentBridge"]);
    run(&["config", "user.email", "test@example.com"]);
    fs::write(dir.path().join("README.md"), "hello\n").unwrap();
    run(&["add", "README.md"]);
    run(&["commit", "-m", "init"]);
    dir
}

#[cfg(windows)]
fn write_fake(dir: &Path, kind: FakeKind) -> PathBuf {
    let (name, body) = match kind {
        FakeKind::Success => ("fake-opencode.cmd", "@echo off\r\nif /I \"%~1\"==\"--version\" (echo opencode 0.0.0-test & exit /b 0)\r\necho OpenCode test> \"%CD%\\TEST.md\"\r\necho created TEST.md\r\necho test result: ok\r\nexit /b 0\r\n"),
        FakeKind::Fail => ("fake-opencode-fail.cmd", "@echo off\r\nif /I \"%~1\"==\"--version\" (echo opencode 0.0.0-test & exit /b 0)\r\necho test result: FAILED\r\nexit /b 2\r\n"),
        FakeKind::Hang => ("fake-opencode-hang.cmd", "@echo off\r\nif /I \"%~1\"==\"--version\" (echo opencode 0.0.0-test & exit /b 0)\r\nping -n 45 127.0.0.1 >nul\r\nexit /b 0\r\n"),
    };
    let path = dir.join(name);
    fs::write(&path, body).unwrap();
    path
}

#[cfg(not(windows))]
fn write_fake(dir: &Path, kind: FakeKind) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let (name, body) = match kind {
        FakeKind::Success => ("fake-opencode", "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo \"opencode 0.0.0-test\"; exit 0; fi\nprintf 'OpenCode test\\n' > TEST.md\necho created TEST.md\necho \"test result: ok\"\nexit 0\n"),
        FakeKind::Fail => ("fake-opencode-fail", "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo \"opencode 0.0.0-test\"; exit 0; fi\necho \"test result: FAILED\"\nexit 2\n"),
        FakeKind::Hang => ("fake-opencode-hang", "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo \"opencode 0.0.0-test\"; exit 0; fi\nsleep 45\nexit 0\n"),
    };
    let path = dir.join(name);
    fs::write(&path, body).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn runtime_for(dir: &Path, command: PathBuf) -> TaskRuntime {
    let mut cfg = Config::new(dir.to_path_buf());
    cfg.executor.kind = "opencode".into();
    cfg.executor.command = command.to_string_lossy().into_owned();
    let storage = common::test_storage();
    let hub = common::hub_with(
        Arc::new(cfg),
        storage,
        vec![common::project_entry("default", dir.to_path_buf())],
    );
    let project = hub.get("default").unwrap();
    (*project.runtime).clone()
}

fn sample_plan() -> PlanInput {
    PlanInput {
        actions: vec!["Inspect the workspace.".into(), "Create TEST.md.".into()],
        tests: vec!["cargo test".into()],
        success_criteria: "TEST.md exists.".into(),
    }
}

#[test]
fn doctor_detects_opencode_yes_and_no() {
    let dir = TempDir::new().unwrap();
    let fake = write_fake(dir.path(), FakeKind::Success);
    let cfg_path = dir.path().join(".agentbridge.toml");

    let mut cfg = Config::new(dir.path().to_path_buf());
    cfg.executor.command = fake.to_string_lossy().into_owned();
    cfg.save_to_path(&cfg_path).unwrap();
    let loaded = Config::load_from_path(&cfg_path).unwrap();

    let checks = doctor::run(Some(&loaded), Some(&cfg_path)).unwrap();
    let oc = checks
        .iter()
        .find(|c| c.name == "opencode")
        .expect("opencode check");
    assert_eq!(oc.status, CheckStatus::Ok, "{}", oc.detail);
    assert!(oc.detail.contains("installed: yes"), "detail={}", oc.detail);

    let mut missing = loaded.clone();
    missing.executor.command = "opencode-not-installed-agentbridge-xyz".into();
    let checks = doctor::run(Some(&missing), Some(&cfg_path)).unwrap();
    let oc = checks
        .iter()
        .find(|c| c.name == "opencode")
        .expect("opencode check");
    assert_eq!(oc.status, CheckStatus::Fail, "{}", oc.detail);
    assert!(oc.detail.contains("installed: no"), "detail={}", oc.detail);
}

#[tokio::test(flavor = "multi_thread")]
async fn task_start_creates_test_md_and_git_diff() {
    if !git::git_available() {
        return;
    }
    let dir = git_workspace();
    let fake = write_fake(dir.path(), FakeKind::Success);
    let runtime = runtime_for(dir.path(), fake);

    let req = StartTaskRequest::new("default", "Create TEST.md in the workspace.", sample_plan());
    let state = runtime.start_task(req).await.unwrap();
    assert_eq!(state.status.as_deref(), Some("running"));
    assert!(state.task_id.is_some());

    let finished = tokio::time::timeout(Duration::from_secs(20), runtime.wait())
        .await
        .expect("executor timed out")
        .unwrap();
    assert_eq!(finished.task_status, Some(TaskStatus::Executed));
    assert_eq!(finished.status.as_deref(), Some("success"));
    assert_eq!(finished.exit_code, Some(0));
    assert!(dir.path().join("TEST.md").is_file());
    assert!(finished
        .changed_files
        .iter()
        .any(|f| f.ends_with("TEST.md")));

    let diff = git::diff(dir.path(), false, 65_536).unwrap();
    assert!(diff.diff.contains("TEST.md"));
}

#[tokio::test(flavor = "multi_thread")]
async fn task_start_missing_opencode_returns_clear_error() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("README.md"), "x\n").unwrap();
    let runtime = runtime_for(
        dir.path(),
        PathBuf::from("opencode-not-installed-agentbridge-xyz"),
    );
    let err = runtime
        .start_task(StartTaskRequest::new("default", "anything", sample_plan()))
        .await
        .unwrap_err();
    assert!(matches!(err, ExecutorError::NotInstalled(_)));
}

#[tokio::test(flavor = "multi_thread")]
async fn task_start_failure_sets_failed_and_nonzero_exit() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("README.md"), "x\n").unwrap();
    let fake = write_fake(dir.path(), FakeKind::Fail);
    let runtime = runtime_for(dir.path(), fake);
    runtime
        .start_task(StartTaskRequest::new(
            "default",
            "this should fail",
            sample_plan(),
        ))
        .await
        .unwrap();
    let finished = tokio::time::timeout(Duration::from_secs(20), runtime.wait())
        .await
        .expect("executor timed out")
        .unwrap();
    assert_eq!(finished.task_status, Some(TaskStatus::Failed));
    assert_eq!(finished.status.as_deref(), Some("failed"));
    assert_ne!(finished.exit_code, Some(0));
}

#[tokio::test(flavor = "multi_thread")]
async fn task_cancel_terminates_opencode() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("README.md"), "x\n").unwrap();
    let fake = write_fake(dir.path(), FakeKind::Hang);
    let runtime = runtime_for(dir.path(), fake);
    let started = runtime
        .start_task(StartTaskRequest::new(
            "default",
            "hang until cancelled",
            sample_plan(),
        ))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    let cancelled = runtime.cancel(started.task_id.as_deref()).await.unwrap();
    assert_eq!(cancelled.task_status, Some(TaskStatus::Cancelled));

    let pid_path = dir.path().join(".agentbridge/executor.pid");
    if pid_path.is_file() {
        let pid: u32 = fs::read_to_string(&pid_path)
            .unwrap()
            .trim()
            .parse()
            .unwrap_or(0);
        assert!(pid == 0 || !executor::process_is_alive(pid));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn two_project_runtimes_run_and_cancel_independently() {
    let project_a = TempDir::new().unwrap();
    let project_b = TempDir::new().unwrap();
    fs::write(project_a.path().join("README.md"), "project A\n").unwrap();
    fs::write(project_b.path().join("README.md"), "project B\n").unwrap();

    let fake_dir = TempDir::new().unwrap();
    let fake = write_fake(fake_dir.path(), FakeKind::Hang);
    let runtime_a = runtime_for(project_a.path(), fake.clone());
    let runtime_b = runtime_for(project_b.path(), fake);

    let started_a = runtime_a
        .start_task(StartTaskRequest::new(
            "default",
            "Keep project A running.",
            sample_plan(),
        ))
        .await
        .unwrap();
    let started_b = runtime_b
        .start_task(StartTaskRequest::new(
            "default",
            "Keep project B running.",
            sample_plan(),
        ))
        .await
        .unwrap();
    assert_ne!(started_a.task_id, started_b.task_id);

    let cancelled_a = runtime_a
        .cancel(started_a.task_id.as_deref())
        .await
        .unwrap();
    assert_eq!(cancelled_a.task_status, Some(TaskStatus::Cancelled));
    assert_eq!(
        runtime_b
            .status(Some(started_b.task_id.as_deref().unwrap()))
            .await
            .unwrap()
            .status
            .as_deref(),
        Some("running")
    );

    let cancelled_b = runtime_b
        .cancel(started_b.task_id.as_deref())
        .await
        .unwrap();
    assert_eq!(cancelled_b.task_status, Some(TaskStatus::Cancelled));
}

fn two_project_service(
    project_a: &Path,
    project_b: &Path,
) -> (TaskService, Arc<agentbridge::storage::Storage>) {
    let storage = common::test_storage();
    let mut cfg = Config::new(project_a.to_path_buf());
    cfg.executor.kind = "opencode".into();
    let hub = Arc::new(common::hub_with(
        Arc::new(cfg),
        storage.clone(),
        vec![
            common::project_entry("alpha", project_a.to_path_buf()),
            common::project_entry("beta", project_b.to_path_buf()),
        ],
    ));
    (TaskService::new(hub, storage.clone()), storage)
}

#[tokio::test(flavor = "multi_thread")]
async fn service_status_is_keyed_by_task_id_across_projects() {
    let project_a = TempDir::new().unwrap();
    let project_b = TempDir::new().unwrap();
    fs::write(project_a.path().join("README.md"), "A\n").unwrap();
    fs::write(project_b.path().join("README.md"), "B\n").unwrap();
    let (service, _storage) = two_project_service(project_a.path(), project_b.path());

    let alpha = service
        .plan_task("alpha", "alpha goal".into(), Vec::new(), "opencode")
        .unwrap();
    let beta = service
        .plan_task("beta", "beta goal".into(), Vec::new(), "opencode")
        .unwrap();
    assert_ne!(alpha.task_id, beta.task_id);

    // Querying alpha's task while naming project beta must follow the task_id.
    let by_id = service
        .get_status("beta", alpha.task_id.as_deref())
        .await
        .unwrap();
    assert_eq!(by_id.task_id, alpha.task_id);
    assert_eq!(by_id.goal.as_deref(), Some("alpha goal"));

    // Without task_id the query stays inside the requested project.
    let scoped = service.get_status("beta", None).await.unwrap();
    assert_eq!(scoped.task_id, beta.task_id);
    assert_eq!(scoped.goal.as_deref(), Some("beta goal"));
}

#[tokio::test(flavor = "multi_thread")]
async fn service_status_unknown_task_id_does_not_fall_back_to_project() {
    let project_a = TempDir::new().unwrap();
    let project_b = TempDir::new().unwrap();
    fs::write(project_a.path().join("README.md"), "A\n").unwrap();
    fs::write(project_b.path().join("README.md"), "B\n").unwrap();
    let (service, _storage) = two_project_service(project_a.path(), project_b.path());

    service
        .plan_task("alpha", "alpha goal".into(), Vec::new(), "opencode")
        .unwrap();
    let err = service
        .get_status("alpha", Some("c2c_does_not_exist"))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("c2c_does_not_exist"), "{err}");
}

#[tokio::test(flavor = "multi_thread")]
async fn service_cancel_targets_task_owner_not_requested_project() {
    let project_a = TempDir::new().unwrap();
    let project_b = TempDir::new().unwrap();
    fs::write(project_a.path().join("README.md"), "A\n").unwrap();
    fs::write(project_b.path().join("README.md"), "B\n").unwrap();

    let fake_dir = TempDir::new().unwrap();
    let fake = write_fake(fake_dir.path(), FakeKind::Hang);

    let storage = common::test_storage();
    let mut cfg = Config::new(project_a.path().to_path_buf());
    cfg.executor.kind = "opencode".into();
    cfg.executor.command = fake.to_string_lossy().into_owned();
    let hub = Arc::new(common::hub_with(
        Arc::new(cfg),
        storage.clone(),
        vec![
            common::project_entry("alpha", project_a.path().to_path_buf()),
            common::project_entry("beta", project_b.path().to_path_buf()),
        ],
    ));
    let service = TaskService::new(hub, storage);

    let started_a = service
        .start_task(StartTaskRequest::new("alpha", "keep A", sample_plan()))
        .await
        .unwrap();
    let started_b = service
        .start_task(StartTaskRequest::new("beta", "keep B", sample_plan()))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Cancel beta's task while naming project alpha: identity follows task_id.
    let cancelled = service
        .cancel_task(CancelTaskRequest {
            project_name: "alpha".into(),
            task_id: started_b.task_id.clone(),
        })
        .await
        .unwrap();
    assert_eq!(cancelled.task_id, started_b.task_id);
    assert_eq!(cancelled.task_status, Some(TaskStatus::Cancelled));

    // Alpha's own task must be untouched.
    let still_running = service.get_status("alpha", None).await.unwrap();
    assert_eq!(still_running.task_id, started_a.task_id);
    assert_eq!(still_running.task_status, Some(TaskStatus::Running));

    service
        .cancel_task(CancelTaskRequest {
            project_name: "alpha".into(),
            task_id: started_a.task_id.clone(),
        })
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn service_same_project_tasks_coexist_and_status_by_id() {
    let project_a = TempDir::new().unwrap();
    let project_b = TempDir::new().unwrap();
    fs::write(project_a.path().join("README.md"), "A\n").unwrap();
    fs::write(project_b.path().join("README.md"), "B\n").unwrap();
    let (service, storage) = two_project_service(project_a.path(), project_b.path());

    let a1 = service
        .plan_task("alpha", "goal A1".into(), Vec::new(), "opencode")
        .unwrap();
    let a2 = service
        .plan_task("alpha", "goal A2".into(), Vec::new(), "opencode")
        .unwrap();
    assert_ne!(a1.task_id, a2.task_id);

    // Both same-project tasks must survive independently in storage.
    let stored = storage.list_task_states("alpha").unwrap();
    assert_eq!(stored.len(), 2, "both alpha tasks must be persisted");

    // task_id addresses exactly one task; project is only a filter.
    let first = service
        .get_status("alpha", a1.task_id.as_deref())
        .await
        .unwrap();
    assert_eq!(first.task_id, a1.task_id);
    assert_eq!(first.goal.as_deref(), Some("goal A1"));

    let second = service
        .get_status("alpha", a2.task_id.as_deref())
        .await
        .unwrap();
    assert_eq!(second.task_id, a2.task_id);
    assert_eq!(second.goal.as_deref(), Some("goal A2"));

    // Project-scoped (no task_id) reads only the latest task of that project.
    let latest = service.get_status("alpha", None).await.unwrap();
    assert_eq!(latest.task_id, a2.task_id);
}

#[tokio::test(flavor = "multi_thread")]
async fn service_same_project_tasks_cancel_independently() {
    let project_a = TempDir::new().unwrap();
    let project_b = TempDir::new().unwrap();
    fs::write(project_a.path().join("README.md"), "A\n").unwrap();
    fs::write(project_b.path().join("README.md"), "B\n").unwrap();
    let (service, _storage) = two_project_service(project_a.path(), project_b.path());

    let a1 = service
        .plan_task("alpha", "goal A1".into(), Vec::new(), "opencode")
        .unwrap();
    let a2 = service
        .plan_task("alpha", "goal A2".into(), Vec::new(), "opencode")
        .unwrap();

    let cancelled = service
        .cancel_task(CancelTaskRequest {
            project_name: "alpha".into(),
            task_id: a1.task_id.clone(),
        })
        .await
        .unwrap();
    assert_eq!(cancelled.task_id, a1.task_id);
    assert_eq!(cancelled.task_status, Some(TaskStatus::Cancelled));

    // Cancelling A1 must not touch its sibling A2.
    let sibling = service
        .get_status("alpha", a2.task_id.as_deref())
        .await
        .unwrap();
    assert_eq!(sibling.task_id, a2.task_id);
    assert_eq!(sibling.task_status, Some(TaskStatus::Planned));
}

#[test]
fn legacy_project_keyed_tasks_are_migrated_without_data_loss() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("agentbridge.db");
    let legacy = agentbridge::state::BridgeState {
        task_id: Some("c2c_legacy_0001".into()),
        goal: Some("legacy goal".into()),
        status: Some("success".into()),
        ..Default::default()
    };
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE tasks (project_name TEXT PRIMARY KEY, state_json TEXT NOT NULL, \
             updated_at DATETIME DEFAULT CURRENT_TIMESTAMP);",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tasks (project_name, state_json) VALUES (?1, ?2)",
            rusqlite::params!["alpha", serde_json::to_string(&legacy).unwrap()],
        )
        .unwrap();
    }

    let storage = agentbridge::storage::Storage::open(&db).unwrap();
    let (project, state) = storage
        .find_task_by_id("c2c_legacy_0001")
        .unwrap()
        .expect("legacy task must survive migration");
    assert_eq!(project, "alpha");
    assert_eq!(state.goal.as_deref(), Some("legacy goal"));

    let latest = storage.load_task_state("alpha").unwrap();
    assert_eq!(latest.task_id.as_deref(), Some("c2c_legacy_0001"));
}

#[tokio::test(flavor = "multi_thread")]
async fn task_start_with_unknown_executor_override_returns_not_found() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("README.md"), "x\n").unwrap();
    let runtime = runtime_for(dir.path(), PathBuf::from("opencode"));
    let req = StartTaskRequest::new("default", "goal", sample_plan())
        .with_executor("unknown-executor-id");
    let err = runtime.start_task(req).await.unwrap_err();
    assert!(matches!(err, ExecutorError::NotFound(_)));
}

#[test]
fn executor_type_allowlist() {
    assert!(executor::validate_executor_type("opencode").is_ok());
    assert!(executor::validate_executor_type("codex").is_err());
    assert!(executor::validate_executor_type("shell").is_err());
}
