use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use walkdir::WalkDir;

use crate::config::DEFAULT_MAX_SEARCH_RESULTS;

const DEFAULT_IGNORE_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".cache",
    ".agentbridge",
];

const HOME_SECRET_DIRS: &[&str] = &[".ssh", ".aws", ".config", ".docker"];
const HOME_SECRET_FILES: &[&str] = &[".npmrc"];

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

    /// Resolve a user-supplied path so it cannot escape the workspace.
    pub fn resolve(&self, requested: &str) -> Result<PathBuf, WorkspaceError> {
        if requested.trim().is_empty() || requested == "." {
            return Ok(self.root.clone());
        }
        if requested.starts_with('~') {
            return Err(WorkspaceError::OutsideWorkspace(requested.to_string()));
        }

        let req = Path::new(requested);
        let combined = if is_os_absolute(req) {
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

fn detect_project_types(root: &Path) -> Vec<String> {
    let mut types = Vec::new();
    if root.join("Cargo.toml").is_file() {
        types.push("rust".into());
    }
    if root.join("package.json").is_file() {
        types.push("node".into());
    }
    if root.join("pyproject.toml").is_file()
        || root.join("requirements.txt").is_file()
        || root.join("setup.py").is_file()
    {
        types.push("python".into());
    }
    if root.join("go.mod").is_file() {
        types.push("go".into());
    }
    if root.join("pom.xml").is_file()
        || root.join("build.gradle").is_file()
        || root.join("build.gradle.kts").is_file()
    {
        types.push("java".into());
    }
    if root.join("CMakeLists.txt").is_file() || has_c_sources(root) {
        types.push("c/c++".into());
    }
    types
}

fn has_c_sources(root: &Path) -> bool {
    fs::read_dir(root).into_iter().flatten().flatten().any(|e| {
        let name = e.file_name();
        let name = name.to_string_lossy();
        name.ends_with(".c")
            || name.ends_with(".cc")
            || name.ends_with(".cpp")
            || name.ends_with(".h")
            || name.ends_with(".hpp")
    })
}

fn is_ignored_dir(name: &str) -> bool {
    DEFAULT_IGNORE_DIRS
        .iter()
        .any(|d| name.eq_ignore_ascii_case(d))
}

fn is_os_absolute(path: &Path) -> bool {
    path.is_absolute()
}

/// Lexically resolve `.` and `..` without touching the filesystem.
fn lexical_normalize(path: &Path) -> Result<PathBuf, WorkspaceError> {
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

fn canonicalize_existing(path: &Path) -> Result<PathBuf, WorkspaceError> {
    let canon = fs::canonicalize(path)?;
    Ok(strip_verbatim(canon))
}

/// Windows `canonicalize` yields `\\?\C:\...`. Strip that so prefix checks work.
fn strip_verbatim(path: PathBuf) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path
    }
}

fn is_within(path: &Path, root: &Path) -> bool {
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

fn component_eq(a: &Component<'_>, b: &Component<'_>) -> bool {
    #[cfg(windows)]
    {
        a.as_os_str().eq_ignore_ascii_case(b.as_os_str())
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

fn relative_display(root: &Path, path: &Path) -> String {
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

fn deny_sensitive(path: &Path, requested: &str) -> Result<(), WorkspaceError> {
    for comp in path.components() {
        if let Component::Normal(name) = comp {
            if is_sensitive_name(&name.to_string_lossy()) {
                return Err(WorkspaceError::Sensitive(requested.to_string()));
            }
        }
    }
    Ok(())
}

fn deny_home_secret_trees(path: &Path, requested: &str) -> Result<(), WorkspaceError> {
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

fn truncate_line(line: &str) -> String {
    const MAX: usize = 240;
    if line.len() <= MAX {
        line.to_string()
    } else {
        format!("{}…", &line[..MAX])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Workspace) {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        fs::write(dir.path().join(".env"), "SECRET=1\n").unwrap();
        fs::write(dir.path().join("id_rsa"), "fake-key\n").unwrap();
        let ws = Workspace::open(dir.path(), 1_048_576, true).unwrap();
        (dir, ws)
    }

    #[test]
    fn read_file_allows_workspace_relative() {
        let (_dir, ws) = setup();
        let text = ws.read_file("src/main.rs").unwrap();
        assert!(text.contains("fn main()"));
    }

    #[test]
    fn read_file_rejects_parent_traversal() {
        let (_dir, ws) = setup();
        let err = ws.read_file("../secret").unwrap_err();
        assert!(matches!(err, WorkspaceError::OutsideWorkspace(_)));
    }

    #[test]
    fn read_file_rejects_nested_parent_traversal() {
        let (_dir, ws) = setup();
        let err = ws.read_file("src/../../secret").unwrap_err();
        assert!(matches!(err, WorkspaceError::OutsideWorkspace(_)));
    }

    #[test]
    fn read_file_rejects_absolute_outside() {
        let (_dir, ws) = setup();
        #[cfg(unix)]
        let outside = "/etc/passwd";
        #[cfg(windows)]
        let outside = r"C:\Windows\System32\drivers\etc\hosts";
        let err = ws.read_file(outside).unwrap_err();
        assert!(
            matches!(
                err,
                WorkspaceError::OutsideWorkspace(_) | WorkspaceError::Sensitive(_)
            ),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn read_file_rejects_tilde_home() {
        let (_dir, ws) = setup();
        let err = ws.read_file("~/.ssh/id_rsa").unwrap_err();
        assert!(matches!(
            err,
            WorkspaceError::OutsideWorkspace(_) | WorkspaceError::Sensitive(_)
        ));
    }

    #[test]
    fn sensitive_env_rejected() {
        let (_dir, ws) = setup();
        let err = ws.read_file(".env").unwrap_err();
        assert!(matches!(err, WorkspaceError::Sensitive(_)));
        let err = ws.read_file("id_rsa").unwrap_err();
        assert!(matches!(err, WorkspaceError::Sensitive(_)));
        assert!(ws.read_file("src/main.rs").is_ok());
    }

    #[test]
    fn sensitive_name_patterns() {
        assert!(is_sensitive_name(".env"));
        assert!(is_sensitive_name(".env.local"));
        assert!(is_sensitive_name("server.pem"));
        assert!(is_sensitive_name("tls.key"));
        assert!(is_sensitive_name("id_rsa"));
        assert!(is_sensitive_name("id_ed25519"));
        assert!(is_sensitive_name("credentials.json"));
        assert!(is_sensitive_name("secrets.yaml"));
        assert!(!is_sensitive_name("src/main.rs"));
        assert!(!is_sensitive_name("main.rs"));
        assert!(!is_sensitive_name("keyboard.rs"));
    }

    #[test]
    fn search_returns_line_numbers_and_ignores_dirs() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::create_dir_all(dir.path().join("target/debug")).unwrap();
        fs::create_dir_all(dir.path().join("node_modules/pkg")).unwrap();
        fs::write(
            dir.path().join("src/lib.rs"),
            "hello SearchTerm world\nsecond line\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("target/debug/out.rs"),
            "SearchTerm should be ignored\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("node_modules/pkg/index.js"),
            "SearchTerm should be ignored\n",
        )
        .unwrap();
        let ws = Workspace::open(dir.path(), 1_048_576, true).unwrap();
        let results = ws.search("SearchTerm", 50).unwrap();
        assert_eq!(results.hits.len(), 1);
        assert_eq!(results.hits[0].file, "src/lib.rs");
        assert_eq!(results.hits[0].line, 1);
        assert!(results.hits[0].text.contains("SearchTerm"));
    }

    #[test]
    fn list_directory_stays_inside_workspace() {
        let (_dir, ws) = setup();
        let listing = ws.list_directory("src").unwrap();
        assert!(listing.entries.iter().any(|e| e.name == "main.rs"));
        assert!(ws.list_directory("..").is_err());
    }

    #[test]
    fn oversized_file_rejected() {
        let dir = TempDir::new().unwrap();
        let mut f = fs::File::create(dir.path().join("big.txt")).unwrap();
        f.write_all(&[b'a'; 64]).unwrap();
        let ws = Workspace::open(dir.path(), 16, true).unwrap();
        let err = ws.read_file("big.txt").unwrap_err();
        assert!(matches!(err, WorkspaceError::TooLarge { .. }));
    }

    #[test]
    fn binary_file_rejected() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("blob.bin"), [0u8, 1, 2, 3]).unwrap();
        let ws = Workspace::open(dir.path(), 1_048_576, true).unwrap();
        let err = ws.read_file("blob.bin").unwrap_err();
        assert!(matches!(err, WorkspaceError::Binary(_)));
    }

    #[test]
    fn detect_rust_project() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]\nname=\"x\"\n").unwrap();
        let ws = Workspace::open(dir.path(), 1_048_576, true).unwrap();
        assert_eq!(ws.info().project_type, vec!["rust".to_string()]);
    }
}
