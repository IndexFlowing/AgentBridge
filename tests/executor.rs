//! OpenCode executor integration tests.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use agentbridge::config::Config;
use agentbridge::doctor::{self, CheckStatus};
use agentbridge::executor::{self, ExecutorError};
use agentbridge::git;
use agentbridge::projects::ProjectHub;
use agentbridge::state::TaskStatus;
use agentbridge::task::{PlanInput, TaskRuntime};
use tempfile::TempDir;

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
    let hub = ProjectHub::single(dir.to_path_buf(), Arc::new(cfg)).unwrap();
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
    let oc = checks.iter().find(|c| c.name == "opencode").expect("opencode check");
    assert_eq!(oc.status, CheckStatus::Ok, "{}", oc.detail);
    assert!(oc.detail.contains("installed: yes"), "detail={}", oc.detail);

    let mut missing = loaded.clone();
    missing.executor.command = "opencode-not-installed-agentbridge-xyz".into();
    let checks = doctor::run(Some(&missing), Some(&cfg_path)).unwrap();
    let oc = checks.iter().find(|c| c.name == "opencode").expect("opencode check");
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

    let state = runtime
        .start_task("Create TEST.md in the workspace.".into(), sample_plan(), None)
        .await
        .unwrap();
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
    assert!(finished.changed_files.iter().any(|f| f.ends_with("TEST.md")));

    let diff = git::diff(dir.path(), false, 65_536).unwrap();
    assert!(diff.diff.contains("TEST.md"));
}

#[tokio::test(flavor = "multi_thread")]
async fn task_start_missing_opencode_returns_clear_error() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("README.md"), "x\n").unwrap();
    let runtime = runtime_for(dir.path(), PathBuf::from("opencode-not-installed-agentbridge-xyz"));
    let err = runtime
        .start_task("anything".into(), sample_plan(), None)
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
        .start_task("this should fail".into(), sample_plan(), None)
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
        .start_task("hang until cancelled".into(), sample_plan(), None)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    let cancelled = runtime.cancel(started.task_id.as_deref()).await.unwrap();
    assert_eq!(cancelled.task_status, Some(TaskStatus::Cancelled));

    let pid_path = dir.path().join(".agentbridge/executor.pid");
    if pid_path.is_file() {
        let pid: u32 = fs::read_to_string(&pid_path).unwrap().trim().parse().unwrap_or(0);
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
        .start_task("Keep project A running.".into(), sample_plan(), None)
        .await
        .unwrap();
    let started_b = runtime_b
        .start_task("Keep project B running.".into(), sample_plan(), None)
        .await
        .unwrap();
    assert_ne!(started_a.task_id, started_b.task_id);

    let cancelled_a = runtime_a.cancel(started_a.task_id.as_deref()).await.unwrap();
    assert_eq!(cancelled_a.task_status, Some(TaskStatus::Cancelled));
    assert_eq!(
        runtime_b.status(Some(started_b.task_id.as_deref().unwrap())).await.unwrap().status.as_deref(),
        Some("running")
    );

    let cancelled_b = runtime_b.cancel(started_b.task_id.as_deref()).await.unwrap();
    assert_eq!(cancelled_b.task_status, Some(TaskStatus::Cancelled));
}

#[tokio::test(flavor = "multi_thread")]
async fn task_start_with_unknown_executor_override_returns_not_found() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("README.md"), "x\n").unwrap();
    let runtime = runtime_for(dir.path(), PathBuf::from("opencode"));
    let err = runtime
        .start_task("goal".into(), sample_plan(), Some("unknown-executor-id"))
        .await
        .unwrap_err();
    assert!(matches!(err, ExecutorError::NotFound(_)));
}

#[test]
fn executor_type_allowlist() {
    assert!(executor::validate_executor_type("opencode").is_ok());
    assert!(executor::validate_executor_type("codex").is_err());
    assert!(executor::validate_executor_type("shell").is_err());
}