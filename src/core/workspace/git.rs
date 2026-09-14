use std::fs;
use std::path::Path;
use std::process::Command;

use serde::Serialize;

use crate::workspace::is_sensitive_name;

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("git is not available on PATH")]
    NotAvailable,
    #[error("not a git repository")]
    NotARepository,
    #[error("git failed: {0}")]
    Command(String),
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GitStatus {
    pub branch: String,
    pub clean: bool,
    pub changed_files: Vec<String>,
    pub staged_files: Vec<String>,
    pub untracked_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GitDiff {
    pub staged: bool,
    pub diff: String,
    pub truncated: bool,
    pub original_bytes: usize,
}

pub fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn git_version() -> Option<String> {
    let output = Command::new("git").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn run_git(workspace: &Path, args: &[&str]) -> Result<String, GitError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(workspace)
        .output()
        .map_err(|_| GitError::NotAvailable)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("not a git repository") {
            return Err(GitError::NotARepository);
        }
        return Err(GitError::Command(stderr.trim().to_string()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub fn is_repository(workspace: &Path) -> bool {
    workspace.join(".git").exists()
        || run_git(workspace, &["rev-parse", "--is-inside-work-tree"])
            .map(|s| s.trim() == "true")
            .unwrap_or(false)
}

pub fn status(workspace: &Path) -> Result<GitStatus, GitError> {
    if !is_repository(workspace) {
        return Err(GitError::NotARepository);
    }
    let branch = run_git(workspace, &["branch", "--show-current"])?
        .trim()
        .to_string();
    let branch = if branch.is_empty() {
        run_git(workspace, &["rev-parse", "--short", "HEAD"])
            .map(|s| format!("detached {}", s.trim()))
            .unwrap_or_else(|_| "HEAD".into())
    } else {
        branch
    };

    let porcelain = run_git(workspace, &["status", "--porcelain=v1"])?;
    let mut changed = Vec::new();
    let mut staged = Vec::new();
    let mut untracked = Vec::new();

    for line in porcelain.lines() {
        if line.len() < 3 {
            continue;
        }
        let x = line.as_bytes()[0] as char;
        let y = line.as_bytes()[1] as char;
        let file = parse_porcelain_path(&line[3..]);
        if x == '?' && y == '?' {
            untracked.push(file);
            continue;
        }
        if x != ' ' && x != '?' {
            staged.push(file.clone());
        }
        if y != ' ' && y != '?' {
            changed.push(file);
        } else if x != ' ' && x != '?' && y == ' ' {
            // staged-only change still counts as a changed file for "dirty"
            if !changed.contains(&file) && !staged.iter().any(|s| s == &file) {
                changed.push(file);
            }
        }
    }

    // Staged-only files belong in changed_files as well (working tree vs HEAD).
    for file in &staged {
        if !changed.contains(file) {
            changed.push(file.clone());
        }
    }

    let clean = changed.is_empty() && staged.is_empty() && untracked.is_empty();
    Ok(GitStatus {
        branch,
        clean,
        changed_files: changed,
        staged_files: staged,
        untracked_files: untracked,
    })
}

fn parse_porcelain_path(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("\"") {
        return rest.trim_end_matches('"').replace("\\\"", "\"");
    }
    if let Some((_, new)) = trimmed.split_once(" -> ") {
        return new.to_string();
    }
    trimmed.to_string()
}

pub fn diff(workspace: &Path, staged: bool, max_bytes: usize) -> Result<GitDiff, GitError> {
    if !is_repository(workspace) {
        return Err(GitError::NotARepository);
    }
    let args: Vec<&str> = if staged {
        vec!["diff", "--cached"]
    } else {
        vec!["diff"]
    };
    let mut raw = run_git(workspace, &args)?;
    if !staged {
        raw.push_str(&untracked_diffs(
            workspace,
            max_bytes.saturating_sub(raw.len()),
        ));
    }
    Ok(truncate_diff(raw, staged, max_bytes))
}

/// `git diff` ignores untracked files. Surface them as new-file diffs so the
/// Brain can review Executor-created files without a `git add`.
fn untracked_diffs(workspace: &Path, remaining: usize) -> String {
    let Ok(st) = status(workspace) else {
        return String::new();
    };
    if st.untracked_files.is_empty() || remaining < 32 {
        return String::new();
    }
    let mut out = String::new();
    for file in st.untracked_files.iter().take(40) {
        if file.contains("..") || Path::new(file).is_absolute() {
            continue;
        }
        let name = Path::new(file)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if is_sensitive_name(&name) {
            continue;
        }
        let path = workspace.join(file);
        let Ok(meta) = fs::metadata(&path) else {
            continue;
        };
        if !meta.is_file() || meta.len() > 256 * 1024 {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        if text.contains('\0') {
            continue;
        }
        out.push_str(&format!(
            "diff --git a/{file} b/{file}\nnew file mode 100644\n--- /dev/null\n+++ b/{file}\n"
        ));
        for line in text.lines() {
            out.push('+');
            out.push_str(line);
            out.push('\n');
        }
        if out.len() >= remaining {
            break;
        }
    }
    out
}

pub fn truncate_diff(raw: String, staged: bool, max_bytes: usize) -> GitDiff {
    let max_bytes = max_bytes.max(64);
    let original_bytes = raw.len();
    if original_bytes <= max_bytes {
        return GitDiff {
            staged,
            diff: raw,
            truncated: false,
            original_bytes,
        };
    }
    let mut end = max_bytes;
    if let Some(idx) = raw[..max_bytes].rfind('\n') {
        end = idx;
    }
    let mut diff = raw[..end].to_string();
    diff.push_str(&format!(
        "\n\n[truncated: showing {end} of {original_bytes} bytes]"
    ));
    GitDiff {
        staged,
        diff,
        truncated: true,
        original_bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

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

    fn repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        git(dir.path(), &["init", "-b", "main"]);
        git(dir.path(), &["config", "user.name", "AgentBridge"]);
        git(dir.path(), &["config", "user.email", "test@example.com"]);
        fs::write(dir.path().join("README.md"), "hello\n").unwrap();
        git(dir.path(), &["add", "README.md"]);
        git(dir.path(), &["commit", "-m", "init"]);
        dir
    }

    #[test]
    fn clean_repository() {
        if !git_available() {
            return;
        }
        let dir = repo();
        let st = status(dir.path()).unwrap();
        assert_eq!(st.branch, "main");
        assert!(st.clean);
        assert!(st.changed_files.is_empty());
        assert!(st.untracked_files.is_empty());
    }

    #[test]
    fn modified_repository() {
        if !git_available() {
            return;
        }
        let dir = repo();
        fs::write(dir.path().join("README.md"), "hello world\n").unwrap();
        fs::write(dir.path().join("new.txt"), "untracked\n").unwrap();
        let st = status(dir.path()).unwrap();
        assert!(!st.clean);
        assert!(st.changed_files.iter().any(|f| f == "README.md"));
        assert!(st.untracked_files.iter().any(|f| f == "new.txt"));
        let d = diff(dir.path(), false, 65_536).unwrap();
        assert!(d.diff.contains("hello world"));
        assert!(
            d.diff.contains("new.txt"),
            "untracked files should appear in git_diff"
        );
        assert!(!d.truncated);
    }

    #[test]
    fn staged_diff() {
        if !git_available() {
            return;
        }
        let dir = repo();
        fs::write(dir.path().join("README.md"), "staged change\n").unwrap();
        git(dir.path(), &["add", "README.md"]);
        let st = status(dir.path()).unwrap();
        assert!(st.staged_files.iter().any(|f| f == "README.md"));
        let d = diff(dir.path(), true, 65_536).unwrap();
        assert!(d.staged);
        assert!(d.diff.contains("staged change"));
    }

    #[test]
    fn diff_truncation() {
        let big = "a".repeat(500);
        let out = truncate_diff(big, false, 100);
        assert!(out.truncated);
        assert!(out.diff.contains("truncated"));
        assert_eq!(out.original_bytes, 500);
        assert!(out.diff.len() < 500);
    }

    #[test]
    fn not_a_repo() {
        if !git_available() {
            return;
        }
        let dir = TempDir::new().unwrap();
        let err = status(dir.path()).unwrap_err();
        assert!(matches!(err, GitError::NotARepository));
    }
}
