// src/core/skill/types.rs
//! Skill domain entities, markdown parser, and directory helpers.

use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillMetadata {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub source: String,
    pub path: PathBuf,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillContent {
    pub metadata: SkillMetadata,
    pub markdown: String,
    pub resources: Vec<String>,
}

pub fn default_skills_dir() -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot locate home directory"))?;
    Ok(home.join(".agentbridge").join("skills"))
}

/// 智能探测真正包含 SKILL.md 的目录（支持直接指定或一层嵌套仓库）
pub fn find_skill_root(source: &Path) -> Option<PathBuf> {
    if source.join("SKILL.md").is_file() {
        return Some(source.to_path_buf());
    }
    if let Ok(entries) = fs::read_dir(source) {
        for entry in entries.flatten() {
            let sub = entry.path();
            if sub.is_dir() && sub.join("SKILL.md").is_file() {
                return Some(sub);
            }
        }
    }
    None
}

/// 从 SKILL.md 正文自动提取标题与第一段描述
pub fn parse_skill_markdown(text: &str, fallback_name: &str) -> (String, String) {
    let mut name = fallback_name.to_string();
    let mut description = String::new();
    let mut lines = text.lines();

    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.starts_with("# ") {
            let title = trimmed.trim_start_matches("# ").trim();
            if !title.is_empty() {
                name = title.to_string();
            }
            break;
        }
    }

    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("```") {
            continue;
        }
        description = trimmed.to_string();
        break;
    }

    if description.is_empty() {
        description = format!("Skill {name}");
    }

    (name, description)
}