// src/workspace/security.rs
//! Path security, sandboxing, traversal denial, and sensitive file protection.

use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::workspace::WorkspaceError;

pub const HOME_SECRET_DIRS: &[&str] = &[".ssh", ".aws", ".config", ".docker"];
pub const HOME_SECRET_FILES: &[&str] = &[".npmrc"];

pub fn is_sensitive_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower == ".env" || lower.starts_with(".env.") {
        return true;
    }
    if lower.ends_with(".pem") || lower.ends_with(".key") {
        return true;
    }
    if lower == "id_rsa" || lower == "id_ed25519" {
        return true;
    }
    if lower.starts_with("credentials") || lower.starts_with("secrets") {
        return true;
    }
    false
}

pub fn deny_sensitive(path: &Path, requested: &str) -> Result<(), WorkspaceError> {
    for comp in path.components() {
        if let Component::Normal(name) = comp {
            if is_sensitive_name(&name.to_string_lossy()) {
                return Err(WorkspaceError::Sensitive(requested.to_string()));
            }
        }
    }
    Ok(())
}

pub fn deny_home_secret_trees(path: &Path, requested: &str) -> Result<(), WorkspaceError> {
    let Some(home) = dirs::home_dir() else {
        return Ok(());
    };
    let home = strip_verbatim(home);
    for dir in HOME_SECRET_DIRS {
        let secret = strip_verbatim(home.join(dir));
        if is_within(path, &secret) {
            return Err(WorkspaceError::Sensitive(requested.to_string()));
        }
    }
    for file in HOME_SECRET_FILES {
        let secret = strip_verbatim(home.join(file));
        if path == secret {
            return Err(WorkspaceError::Sensitive(requested.to_string()));
        }
    }
    Ok(())
}

pub fn lexical_normalize(path: &Path) -> Result<PathBuf, WorkspaceError> {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::Prefix(p) => out.push(p.as_os_str()),
            Component::RootDir => out.push(comp.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => match out.components().next_back() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                _ => {
                    return Err(WorkspaceError::OutsideWorkspace(path.display().to_string()));
                }
            },
            Component::Normal(c) => {
                if c.is_empty() {
                    return Err(WorkspaceError::Invalid(path.display().to_string()));
                }
                out.push(c);
            }
        }
    }
    Ok(out)
}

pub fn canonicalize_existing(path: &Path) -> Result<PathBuf, WorkspaceError> {
    let canon = fs::canonicalize(path)?;
    Ok(strip_verbatim(canon))
}

pub fn strip_verbatim(path: PathBuf) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path
    }
}

pub fn is_within(path: &Path, root: &Path) -> bool {
    let path = strip_verbatim(path.to_path_buf());
    let root = strip_verbatim(root.to_path_buf());
    let path_c: Vec<_> = path.components().collect();
    let root_c: Vec<_> = root.components().collect();
    if path_c.len() < root_c.len() {
        return false;
    }
    path_c
        .iter()
        .zip(root_c.iter())
        .all(|(a, b)| component_eq(a, b))
}

pub fn component_eq(a: &Component<'_>, b: &Component<'_>) -> bool {
    #[cfg(windows)]
    {
        a.as_os_str().eq_ignore_ascii_case(b.as_os_str())
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

pub fn relative_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|p| {
            if p.as_os_str().is_empty() {
                ".".to_string()
            } else {
                p.to_string_lossy().replace('\\', "/")
            }
        })
        .unwrap_or_else(|_| path.display().to_string())
}
