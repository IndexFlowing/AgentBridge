use std::path::PathBuf;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProjectEntry {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub readonly: bool,
    #[serde(default = "default_project_executor")]
    pub executor: String,
}

pub fn default_project_executor() -> String {
    "opencode".into()
}

pub fn validate_project_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        bail!("project name must not be empty");
    }
    if name.len() > 64 {
        bail!("project name is too long");
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_alphanumeric() {
        bail!("project name `{name}` must start with a letter or digit");
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')) {
        bail!("project name `{name}` may only contain letters, digits, '.', '-' and '_'");
    }
    Ok(name.to_string())
}

pub fn project_id(entry: &ProjectEntry) -> String {
    if !entry.id.trim().is_empty() {
        return entry.id.trim().to_string();
    }
    let mut hash = Sha256::new();
    hash.update(entry.name.trim().as_bytes());
    hash.update([0]);
    hash.update(entry.path.to_string_lossy().as_bytes());
    let digest = format!("{:x}", hash.finalize());
    format!("project-{}", &digest[..24])
}