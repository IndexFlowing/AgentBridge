//! OpenCode executor integration tests for V0.2.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use agentbridge::config::Config;
use agentbridge::doctor::{self, CheckStatus};
use agentbridge::executor::{self, ExecutorError};
use agentbridge::git;
use agentbridge::state::TaskStatus;
use agentbridge::task::{PlanInput, TaskRuntime};
use agentbridge::workspace::Workspace;
use tempfile::TempDir;

#[derive(Clone, Copy)]
enum FakeKind {
    Success,
    Fail,
    Hang,
}

fn git_ok() -> bool {
    git::git_available()
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "AgentBridge")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "AgentBridge")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

fn git_workspace() -> TempDir {
    let dir = TempDir::new().unwrap();
    git(dir.path(), &["init", "-b", "main"]);
    git(dir.path(), &["config", "user.name", "AgentBridge"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    fs::write(dir.path().join("README.md"), "hello\n").unwrap();
    git(dir.path(), &["add", "README.md"]);
    git(dir.path(), &["commit", "-m", "init"]);
    dir
}

fn write_fake(dir: &Path, kind: FakeKind) -> PathBuf {
    #[cfg(windows)]
    {
        let path = dir.join(match kind {
            FakeKind::Success => "fake-opencode.cmd",
            FakeKind::Fail => "fake-opencode-fail.cmd",
            FakeKind::Hang => "fake-opencode-hang.cmd",
        });
        let body = match kind {
            FakeKind::Success => {
                r#"@echo off
if /I "%~1"=="--version" (
  echo opencode 0.0.0-test
  exit /b 0
)
echo OpenCode test executor> "%CD%\TEST.md"
echo created TEST.md
echo test result: ok
exit /b 0
"#
            }
            FakeKind::Fail => {
                r#"@echo off
if /I "%~1"=="--version" (
  echo opencode 0.0.0-test
  exit /b 0
)
echo OpenCode failed
echo test result: FAILED
exit /b 2
"#
            }
            FakeKind::Hang => {
                r#"@echo off
if /I "%~1"=="--version" (
  echo opencode 0.0.0-test
  exit /b 0
)
ping -n 45 127.0.0.1 >nul
exit /b 0
"#
            }
        };
        fs::write(&path, body).unwrap();
        path
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(match kind {
            FakeKind::Success => "fake-opencode",
            FakeKind::Fail => "fake-opencode-fail",
            FakeKind::Hang => "fake-opencode-hang",
        });
        let body = match kind {
            FakeKind::Success => {
                r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "opencode 0.0.0-test"
  exit 0
fi
printf 'OpenCode test executor\n' > TEST.md
echo created TEST.md
echo "test result: ok"
exit 0
"#
            }
            FakeKind::Fail => {
                r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "opencode 0.0.0-test"
  exit 0
fi
echo OpenCode failed
echo "test result: FAILED"
exit 2
"#
            }
            FakeKind::Hang => {
                r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "opencode 0.0.0-test"
  exit 0
fi
sleep 45
exit 0
"#
            }
        };
        fs::write(&path, body).unwrap();
        let mut perm = fs::metadata(&path).unwrap().permissions();
        perm.set_mode(0o755);
        fs::set_permissions(&path, perm).unwrap();
        path
    }
}

fn runtime_for(dir: &Path, command: PathBuf) -> TaskRuntime {
    let mut cfg = Config::new(dir.to_path_buf());
    cfg.executor.kind = "opencode".into();
    cfg.executor.command = command.to_string_lossy().into_owned();
    let ws = Workspace::open(dir, 1_048_576, true).unwrap();
    TaskRuntime::new(Arc::new(ws), Arc::new(cfg)).unwrap()
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
    assert!(
        oc.detail.contains("installed: yes"),
        "expected installed: yes, got {}",
        oc.detail
    );

    let mut missing = loaded.clone();
    missing.executor.command = "opencode-not-installed-agentbridge-xyz".into();
    let checks = doctor::run(Some(&missing), Some(&cfg_path)).unwrap();
    let oc = checks
        .iter()
        .find(|c| c.name == "opencode")
        .expect("opencode check");
    assert_eq!(oc.status, CheckStatus::Fail, "{}", oc.detail);
    assert!(
        oc.detail.contains("installed: no"),
        "expected installed: no, got {}",
        oc.detail
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn task_start_creates_test_md_and_git_diff() {
    if !git_ok() {
        return;
    }
    let dir = git_workspace();
    let fake = write_fake(dir.path(), FakeKind::Success);
    let runtime = runtime_for(dir.path(), fake);

    let state = runtime
        .start_task("Create TEST.md in the workspace.".into(), sample_plan())
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
    assert!(
        dir.path().join("TEST.md").is_file(),
        "OpenCode should have created TEST.md"
    );
    assert!(
        finished
            .changed_files
            .iter()
            .any(|f| f == "TEST.md" || f.ends_with("TEST.md")),
        "changed_files: {:?}",
        finished.changed_files
    );

    let snapshot = runtime.status(finished.task_id.as_deref()).await.unwrap();
    let payload = snapshot.task_status_payload();
    assert_eq!(payload["status"], "success");
    assert_eq!(payload["exit_code"], 0);

    let diff = git::diff(dir.path(), false, 65_536).unwrap();
    let st = git::status(dir.path()).unwrap();
    assert!(
        st.untracked_files.iter().any(|f| f == "TEST.md")
            || st.changed_files.iter().any(|f| f == "TEST.md"),
        "expected TEST.md in git status, status={st:?}"
    );
    assert!(
        diff.diff.contains("TEST.md"),
        "expected TEST.md in git_diff after executor, diff={}",
        diff.diff
    );
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
        .start_task("anything".into(), sample_plan())
        .await
        .unwrap_err();
    assert!(matches!(err, ExecutorError::NotInstalled(_)));
    assert!(
        err.to_string().to_lowercase().contains("not installed"),
        "{err}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn task_start_failure_sets_failed_and_nonzero_exit() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("README.md"), "x\n").unwrap();
    let fake = write_fake(dir.path(), FakeKind::Fail);
    let runtime = runtime_for(dir.path(), fake);
    runtime
        .start_task("this should fail".into(), sample_plan())
        .await
        .unwrap();
    let finished = tokio::time::timeout(Duration::from_secs(20), runtime.wait())
        .await
        .expect("executor timed out")
        .unwrap();
    assert_eq!(finished.task_status, Some(TaskStatus::Failed));
    assert_eq!(finished.status.as_deref(), Some("failed"));
    assert_ne!(finished.exit_code, Some(0));
    assert!(finished.exit_code.unwrap_or(0) != 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn task_cancel_terminates_opencode() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("README.md"), "x\n").unwrap();
    let fake = write_fake(dir.path(), FakeKind::Hang);
    let runtime = runtime_for(dir.path(), fake);
    let started = runtime
        .start_task("hang until cancelled".into(), sample_plan())
        .await
        .unwrap();
    assert_eq!(started.status.as_deref(), Some("running"));
    tokio::time::sleep(Duration::from_millis(200)).await;

    let cancelled = runtime.cancel(started.task_id.as_deref()).await.unwrap();
    assert_eq!(cancelled.task_status, Some(TaskStatus::Cancelled));
    assert_eq!(cancelled.status.as_deref(), Some("cancelled"));

    let pid_path = dir.path().join(".agentbridge").join("executor.pid");
    if pid_path.is_file() {
        let pid: u32 = fs::read_to_string(&pid_path)
            .unwrap()
            .trim()
            .parse()
            .unwrap_or(0);
        assert!(
            pid == 0 || !executor::process_is_alive(pid),
            "OpenCode pid {pid} still alive after cancel"
        );
    }
}

#[test]
fn executor_type_allowlist() {
    assert!(executor::validate_executor_type("opencode").is_ok());
    assert!(executor::validate_executor_type("codex").is_err());
    assert!(executor::validate_executor_type("shell").is_err());
}
