//! Path isolation and sensitive-file tests as specified for V0.1.

use agentbridge::mcp::eval_tool;
use agentbridge::workspace::Workspace;
use serde_json::json;
use std::fs;
use tempfile::TempDir;

fn workspace() -> (TempDir, Workspace) {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(dir.path().join(".env"), "SECRET=1\n").unwrap();
    fs::write(dir.path().join("id_rsa"), "fake\n").unwrap();
    let ws = Workspace::open(dir.path(), 1_048_576, true).unwrap();
    (dir, ws)
}

#[test]
fn read_file_src_main_allowed() {
    let (_dir, ws) = workspace();
    let value = eval_tool(&ws, "read_file", json!({"path": "src/main.rs"})).unwrap();
    assert!(value["content"].as_str().unwrap().contains("fn main()"));
}

#[test]
fn read_file_parent_secret_rejected() {
    let (_dir, ws) = workspace();
    let err = eval_tool(&ws, "read_file", json!({"path": "../secret"})).unwrap_err();
    assert!(err.contains("outside the workspace"), "{err}");
}

#[test]
fn read_file_absolute_passwd_rejected() {
    let (_dir, ws) = workspace();
    #[cfg(unix)]
    let path = "/etc/passwd";
    #[cfg(windows)]
    let path = r"C:\Windows\System32\drivers\etc\hosts";
    let err = eval_tool(&ws, "read_file", json!({"path": path})).unwrap_err();
    assert!(
        err.contains("outside the workspace") || err.contains("sensitive"),
        "{err}"
    );
}

#[test]
fn sensitive_env_rejected_main_allowed() {
    let (_dir, ws) = workspace();
    let env_err = eval_tool(&ws, "read_file", json!({"path": ".env"})).unwrap_err();
    assert!(env_err.contains("sensitive"), "{env_err}");
    let key_err = eval_tool(&ws, "read_file", json!({"path": "id_rsa"})).unwrap_err();
    assert!(key_err.contains("sensitive"), "{key_err}");
    eval_tool(&ws, "read_file", json!({"path": "src/main.rs"})).unwrap();
}
