// src/core/skill/service.rs
//! Application Service orchestrating Skill lifecycle, installation, and project policies.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;

use crate::core::skill::provider::{LocalFilesystemSkillProvider, SkillProvider, SkillProviderError};
use crate::core::skill::registry::{reload_skill_registry, shared_skill_registry, SharedSkillRegistry, SkillRegistry};
use crate::core::skill::resolver::SkillResolver;
use crate::core::skill::types::{default_skills_dir, find_skill_root, parse_skill_markdown, SkillMetadata};
use crate::infra::storage::skills::StoredSkillRecord;
use crate::models::{InstallSkillRequest, SkillData, SkillDetailData};
use crate::storage::Storage;

#[derive(Debug, Error)]
pub enum SkillServiceError {
    #[error("Skill '{0}' not found")]
    NotFound(String),
    #[error("Source folder '{0}' is invalid or missing SKILL.md")]
    InvalidSource(String),
    #[error("Skill '{0}' is already installed")]
    AlreadyInstalled(String),
    #[error(transparent)]
    Provider(#[from] SkillProviderError),
    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}

#[derive(Clone)]
pub struct SkillService {
    storage: Arc<Storage>,
    registry: SharedSkillRegistry,
    provider: Arc<dyn SkillProvider>,
}

impl SkillService {
    pub fn new(storage: Arc<Storage>) -> Self {
        let initial = SkillRegistry::from_storage(&storage).unwrap_or_default();
        let registry = shared_skill_registry(initial);
        let provider = Arc::new(LocalFilesystemSkillProvider);
        Self {
            storage,
            registry,
            provider,
        }
    }

    pub fn list_skills(&self) -> Result<Vec<SkillData>, SkillServiceError> {
        let records = self.storage.load_skills()?;
        Ok(records.into_iter().map(SkillData::from).collect())
    }

    pub fn get_skill_detail(&self, name_or_id: &str) -> Result<SkillDetailData, SkillServiceError> {
        let reg = self.registry.read().unwrap().clone();
        let meta = reg
            .get_by_name(name_or_id)
            .or_else(|| reg.get_by_id(name_or_id))
            .ok_or_else(|| SkillServiceError::NotFound(name_or_id.to_string()))?;

        let content = self.provider.read_content(meta)?;
        Ok(SkillDetailData {
            id: meta.id.clone(),
            name: meta.name.clone(),
            description: meta.description.clone(),
            version: meta.version.clone(),
            source: meta.source.clone(),
            path: meta.path.display().to_string(),
            enabled: meta.enabled,
            content: content.markdown,
            resources: content.resources,
        })
    }

    pub fn install_skill(&self, req: InstallSkillRequest) -> Result<SkillData, SkillServiceError> {
        let input_path = PathBuf::from(req.source.trim());
        let real_root = find_skill_root(&input_path)
            .ok_or_else(|| SkillServiceError::InvalidSource(req.source.clone()))?;

        let skill_md_content = fs::read_to_string(real_root.join("SKILL.md"))
            .map_err(|e| SkillServiceError::InvalidSource(e.to_string()))?;

        let folder_name = real_root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "custom-skill".into());

        let (parsed_name, description) = parse_skill_markdown(&skill_md_content, &folder_name);
        let skill_name = req.name_override.unwrap_or(parsed_name).trim().to_string();

        if let Ok(Some(_)) = self.storage.get_skill_by_name(&skill_name) {
            return Err(SkillServiceError::AlreadyInstalled(skill_name));
        }

        let target_dir = default_skills_dir()?.join(&skill_name);
        if !target_dir.exists() {
            copy_dir_all(&real_root, &target_dir)?;
        }

        let id = uuid::Uuid::new_v4().to_string();
        let record = StoredSkillRecord {
            id: id.clone(),
            name: skill_name,
            description,
            version: "1.0.0".into(),
            source: req.source,
            path: target_dir,
            enabled: true,
            installed_at: String::new(),
            updated_at: String::new(),
        };

        self.storage.upsert_skill(record)?;
        reload_skill_registry(&self.registry, &self.storage)?;

        let saved = self.storage.load_skills()?.into_iter().find(|s| s.id == id).unwrap();
        Ok(SkillData::from(saved))
    }

    pub fn set_enabled(&self, name: &str, enabled: bool) -> Result<(), SkillServiceError> {
        self.storage.set_skill_enabled(name, enabled)?;
        reload_skill_registry(&self.registry, &self.storage)?;
        Ok(())
    }

    pub fn remove_skill(&self, name: &str) -> Result<(), SkillServiceError> {
        let reg = self.registry.read().unwrap().clone();
        let meta = reg
            .get_by_name(name)
            .ok_or_else(|| SkillServiceError::NotFound(name.to_string()))?;

        if meta.path.exists() {
            let _ = fs::remove_dir_all(&meta.path);
        }
        self.storage.delete_skill(&meta.id)?;
        reload_skill_registry(&self.registry, &self.storage)?;
        Ok(())
    }

    pub fn resolve_candidates(&self, project_name: Option<&str>, query: &str) -> Vec<SkillMetadata> {
        let reg = self.registry.read().unwrap().clone();
        let all = reg.list();

        let disabled_set = if let Some(proj) = project_name {
            self.storage
                .load_project_skill_policies(proj)
                .unwrap_or_default()
                .into_iter()
                .filter(|p| !p.enabled)
                .map(|p| p.skill_id)
                .collect()
        } else {
            std::collections::HashSet::new()
        };

        let available: Vec<&SkillMetadata> = all
            .into_iter()
            .filter(|s| s.enabled && !disabled_set.contains(&s.id))
            .collect();

        SkillResolver::resolve_candidates(&available, query)
            .into_iter()
            .cloned()
            .collect()
    }
}

fn copy_dir_all(src: &Path, dst: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dest_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dest_path)?;
        } else {
            fs::copy(entry.path(), dest_path)?;
        }
    }
    Ok(())
}