// src/models/skills.rs
//! Domain DTOs for Skill management, installation, and inspection.

use serde::{Deserialize, Serialize};
use crate::infra::storage::skills::StoredSkillRecord;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillData {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub source: String,
    pub path: String,
    pub enabled: bool,
    pub installed_at: String,
    pub updated_at: String,
}

impl From<StoredSkillRecord> for SkillData {
    fn from(r: StoredSkillRecord) -> Self {
        Self {
            id: r.id,
            name: r.name,
            description: r.description,
            version: r.version,
            source: r.source,
            path: r.path.to_string_lossy().to_string(),
            enabled: r.enabled,
            installed_at: r.installed_at,
            updated_at: r.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDetailData {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub source: String,
    pub path: String,
    pub enabled: bool,
    pub content: String,
    pub resources: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallSkillRequest {
    pub source: String,
    #[serde(default)]
    pub name_override: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetSkillPolicyRequest {
    pub project_name: String,
    pub skill_name: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveSkillsRequest {
    pub project_name: Option<String>,
    pub query: String,
}