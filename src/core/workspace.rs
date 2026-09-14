// src/workspace.rs
//! Workspace: Sandboxed local folder operations and path security.

pub mod git;
pub mod inspect;
pub mod security;

pub use git::*;
pub use inspect::*;
pub use security::*;

use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::config::DEFAULT_MAX_SEARCH_RESULTS;

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("path is outside the workspace: {0}")]
    OutsideWorkspace(String),
    #[error("access denied to sensitive path: {0}")]
    Sensitive(String),
    #[error("file is too large ({size} bytes; max {max} bytes): {path}")]
    TooLarge { path: String, size: u64, max: u64 },
    #[error("file is not valid UTF-8 text: {0}")]
    NotUtf8(String),
    #[error("file appears to be binary: {0}")]
    Binary(String),
    #[error("path not found: {0}")]
    NotFound(String),
    #[error("not a directory: {0}")]
    NotDirectory(String),
    #[error("not a file: {0}")]
    NotFile(String),
    #[error("invalid path: {0}")]
    Invalid(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub struct Workspace {
    root: PathBuf,
    max_file_size: u64,
    deny_sensitive_files: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceInfo {
    pub workspace: String,
    pub project_type: Vec<String>,
    pub git_repository: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DirListing {
    pub path: String,
    pub entries: Vec<DirEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DirEntry {
    pub name: String,
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SearchHit {
    pub file: String,
    pub line: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResults {
    pub query: String,
    pub hits: Vec<SearchHit>,
    pub truncated: bool,
}

impl Workspace {
    pub fn open(
        root: impl AsRef<Path>,
        max_file_size: u64,
        deny_sensitive_files: bool,
    ) -> Result<Self, WorkspaceError> {
        let root = root.as_ref();
        if !root.exists() {
            return Err(WorkspaceError::NotFound(root.display().to_string()));
        }
        if !root.is_dir() {
            return Err(WorkspaceError::NotDirectory(root.display().to_string()));
        }
        let root = canonicalize_existing(root)?;
        Ok(Self {
            root,
            max_file_size,
            deny_sensitive_files,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn info(&self) -> WorkspaceInfo {
        WorkspaceInfo {
            workspace: self.root.display().to_string(),
            project_type: detect_project_types(&self.root),
            git_repository: self.root.join(".git").exists(),
        }
    }

    pub fn resolve(&self, requested: &str) -> Result<PathBuf, WorkspaceError> {
        if requested.trim().is_empty() || requested == "." {
            return Ok(self.root.clone());
        }
        if requested.starts_with('~') {
            return Err(WorkspaceError::OutsideWorkspace(requested.to_string()));
        }

        let req = Path::new(requested);
        let combined = if req.is_absolute() {
            req.to_path_buf()
        } else {
            self.root.join(req)
        };

        let normalized = lexical_normalize(&combined)?;
        if !is_within(&normalized, &self.root) {
            return Err(WorkspaceError::OutsideWorkspace(requested.to_string()));
        }

        if self.deny_sensitive_files {
            deny_sensitive(&normalized, requested)?;
            deny_home_secret_trees(&normalized, requested)?;
        }

        if normalized.exists() {
            let canon = canonicalize_existing(&normalized)?;
            if !is_within(&canon, &self.root) {
                return Err(WorkspaceError::OutsideWorkspace(requested.to_string()));
            }
            if self.deny_sensitive_files {
                deny_sensitive(&canon, requested)?;
                deny_home_secret_trees(&canon, requested)?;
            }
            return Ok(canon);
        }

        Ok(normalized)
    }

    pub fn list_directory(&self, path: &str) -> Result<DirListing, WorkspaceError> {
        let resolved = self.resolve(path)?;
        if !resolved.exists() {
            return Err(WorkspaceError::NotFound(path.to_string()));
        }
        if !resolved.is_dir() {
            return Err(WorkspaceError::NotDirectory(path.to_string()));
        }

        let mut entries = Vec::new();
        let mut children: Vec<_> = fs::read_dir(&resolved)?.filter_map(|e| e.ok()).collect();
        children.sort_by_key(|e| e.file_name());

        for child in children {
            let name = child.file_name().to_string_lossy().into_owned();
            let file_type = child.file_type().ok();
            let kind = if file_type.is_some_and(|t| t.is_dir()) {
                "directory"
            } else if file_type.is_some_and(|t| t.is_symlink()) {
                "symlink"
            } else {
                "file"
            };
            let size = if kind == "file" {
                child.metadata().ok().map(|m| m.len())
            } else {
                None
            };
            entries.push(DirEntry { name, kind, size });
        }

        Ok(DirListing {
            path: relative_display(&self.root, &resolved),
            entries,
        })
    }

    pub fn read_file(&self, path: &str) -> Result<String, WorkspaceError> {
        let resolved = self.resolve(path)?;
        if !resolved.exists() {
            return Err(WorkspaceError::NotFound(path.to_string()));
        }
        if resolved.is_dir() {
            return Err(WorkspaceError::NotFile(path.to_string()));
        }
        let meta = fs::metadata(&resolved)?;
        if meta.len() > self.max_file_size {
            return Err(WorkspaceError::TooLarge {
                path: path.to_string(),
                size: meta.len(),
                max: self.max_file_size,
            });
        }
        let bytes = fs::read(&resolved)?;
        if bytes.contains(&0) {
            return Err(WorkspaceError::Binary(path.to_string()));
        }
        String::from_utf8(bytes).map_err(|_| WorkspaceError::NotUtf8(path.to_string()))
    }

    pub fn search(&self, query: &str, max_results: usize) -> Result<SearchResults, WorkspaceError> {
        if query.is_empty() {
            return Err(WorkspaceError::Invalid(
                "search query must not be empty".into(),
            ));
        }
        let cap = max_results.clamp(1, 200);
        let mut hits = Vec::new();
        let mut truncated = false;

        let walker = WalkDir::new(&self.root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                if e.file_type().is_dir() {
                    let name = e.file_name().to_string_lossy();
                    !is_ignored_dir(&name)
                } else {
                    true
                }
            });

        for entry in walker.flatten() {
            if !entry.file_type().is_file() {
                continue;
            }
            if self.deny_sensitive_files && is_sensitive_name(&entry.file_name().to_string_lossy())
            {
                continue;
            }
            let path = entry.path();
            let Ok(meta) = entry.metadata() else { continue };
            if meta.len() > self.max_file_size {
                continue;
            }
            let Ok(bytes) = fs::read(path) else { continue };
            if bytes.contains(&0) {
                continue;
            }
            let Ok(text) = String::from_utf8(bytes) else {
                continue;
            };

            let rel = relative_display(&self.root, path);
            for (idx, line) in text.lines().enumerate() {
                if line.contains(query) {
                    hits.push(SearchHit {
                        file: rel.clone(),
                        line: idx + 1,
                        text: truncate_line(line),
                    });
                    if hits.len() >= cap {
                        truncated = true;
                        break;
                    }
                }
            }
            if truncated {
                break;
            }
        }

        Ok(SearchResults {
            query: query.to_string(),
            hits,
            truncated,
        })
    }
}

pub fn default_search_limit() -> usize {
    DEFAULT_MAX_SEARCH_RESULTS
}
