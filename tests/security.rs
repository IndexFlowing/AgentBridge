//! Path isolation and sensitive-file tests for the read-only workspace sandbox.

use agentbridge::workspace::Workspace;
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
    let value = ws.read_file("src/main.rs").unwrap();
    assert!(value.contains("fn main()"));
}

#[test]
fn read_file_parent_secret_rejected() {
    let (_dir, ws) = workspace();
    let err = ws.read_file("../secret").unwrap_err().to_string();
    assert!(err.contains("outside the workspace"), "{err}");
}

#[test]
fn read_file_absolute_passwd_rejected() {
    let (_dir, ws) = workspace();
    #[cfg(unix)]
    let path = "/etc/passwd";
    #[cfg(windows)]
    let path = r"C:\Windows\System32\drivers\etc\hosts";
    let err = ws.read_file(path).unwrap_err().to_string();
    assert!(
        err.contains("outside the workspace") || err.contains("sensitive"),
        "{err}"
    );
}

#[test]
fn sensitive_env_rejected_main_allowed() {
    let (_dir, ws) = workspace();
    let env_err = ws.read_file(".env").unwrap_err().to_string();
    assert!(env_err.contains("sensitive"), "{env_err}");
    let key_err = ws.read_file("id_rsa").unwrap_err().to_string();
    assert!(key_err.contains("sensitive"), "{key_err}");
    ws.read_file("src/main.rs").unwrap();
}
