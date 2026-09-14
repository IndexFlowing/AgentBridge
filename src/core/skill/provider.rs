// src/core/skill/provider.rs
//! Pluggable Skill Content Provider abstraction.

use std::fs;
use thiserror::Error;

use crate::core::skill::types::{SkillContent, SkillMetadata};

#[derive(Debug, Error)]
pub enum SkillProviderError {
    #[error("Skill folder '{0}' does not exist")]
    NotFound(String),
    #[error("SKILL.md not found in '{0}'")]
    MissingSkillMd(String),
    #[error("I/O error reading skill: {0}")]
    Io(#[from] std::io::Error),
}

pub trait SkillProvider: Send + Sync {
    fn read_content(&self, meta: &SkillMetadata) -> Result<SkillContent, SkillProviderError>;
}

#[derive(Default, Clone)]
pub struct LocalFilesystemSkillProvider;

impl SkillProvider for LocalFilesystemSkillProvider {
    fn read_content(&self, meta: &SkillMetadata) -> Result<SkillContent, SkillProviderError> {
        let path = &meta.path;
        if !path.is_dir() {
            return Err(SkillProviderError::NotFound(path.display().to_string()));
        }
        let skill_md = path.join("SKILL.md");
        if !skill_md.is_file() {
            return Err(SkillProviderError::MissingSkillMd(path.display().to_string()));
        }
        let markdown = fs::read_to_string(&skill_md)?;
        let mut resources = Vec::new();

        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name != "SKILL.md" && !name.starts_with('.') {
                    resources.push(name);
                }
            }
        }
        resources.sort();

        Ok(SkillContent {
            metadata: meta.clone(),
            markdown,
            resources,
        })
    }
}