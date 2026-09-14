// src/workspace/inspect.rs
//! Project language/framework detection and directory filters.

use std::fs;
use std::path::Path;

pub const DEFAULT_IGNORE_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".cache",
    ".agentbridge",
];

pub fn detect_project_types(root: &Path) -> Vec<String> {
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

pub fn has_c_sources(root: &Path) -> bool {
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

pub fn is_ignored_dir(name: &str) -> bool {
    DEFAULT_IGNORE_DIRS
        .iter()
        .any(|d| name.eq_ignore_ascii_case(d))
}

pub fn truncate_line(line: &str) -> String {
    const MAX: usize = 240;
    if line.len() <= MAX {
        line.to_string()
    } else {
        format!("{}…", &line[..MAX])
    }
}