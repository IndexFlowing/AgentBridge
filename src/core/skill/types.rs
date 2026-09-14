// src/core/skill/types.rs
//! Skill domain entities, markdown parser, and directory helpers.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

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
pub fn parse_skill_markdown(text: &str, fallback_name: &str) -> (String, String, String) {
    let mut name = fallback_name.to_string();
    let mut description = String::new();
    let mut version = "1.0.0".to_string();

    let mut lines = text.lines().peekable();

    // 1. 尝试解析 YAML Frontmatter
    if let Some(&first) = lines.peek() {
        if first.trim() == "---" {
            lines.next(); // 吞掉开头的 ---
            while let Some(line) = lines.next() {
                let trimmed = line.trim();
                if trimmed == "---" {
                    break; // Frontmatter 结束
                }
                if let Some((key, val)) = trimmed.split_once(':') {
                    let key = key.trim();
                    let val = val.trim().trim_matches('"').trim_matches('\'').trim();
                    match key {
                        "name" => name = val.to_string(),
                        "description" => description = val.to_string(),
                        "version" => version = val.to_string(),
                        _ => {}
                    }
                }
            }
        }
    }

    // 2. 如果 Frontmatter 里没有提供 name 或 description，尝试从 Markdown 正文提取
    let mut found_h1 = false;
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("```") {
            continue;
        }

        // 抓取第一个 H1 作为备用名称（如果 frontmatter 里没写）
        if trimmed.starts_with("# ") {
            if name == fallback_name && !found_h1 {
                let title = trimmed.trim_start_matches("# ").trim();
                if !title.is_empty() {
                    name = title.to_string();
                }
            }
            found_h1 = true;
            continue;
        }

        // 抓取第一段正文作为备用描述
        if description.is_empty() && !trimmed.starts_with('#') {
            description = trimmed.to_string();
        }

        // 如果都已经拿到了，就没必要继续扫描几千行的文件了
        if name != fallback_name && !description.is_empty() {
            break;
        }
    }

    if description.is_empty() {
        description = format!("Skill {name}");
    }

    (name, description, version)
}
