// src/core/skill/service.rs
//! Application Service orchestrating Skill lifecycle, installation, and project policies.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile;
use thiserror::Error;

use crate::core::skill::provider::{
    LocalFilesystemSkillProvider, SkillProvider, SkillProviderError,
};
use crate::core::skill::registry::{
    reload_skill_registry, shared_skill_registry, SharedSkillRegistry, SkillRegistry,
};
use crate::core::skill::resolver::SkillResolver;
use crate::core::skill::types::{find_skill_root, parse_skill_markdown, SkillMetadata};
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
    skills_dir: PathBuf,
}

impl SkillService {
    pub fn new(storage: Arc<Storage>, skills_dir: PathBuf) -> Self {
        let initial = SkillRegistry::from_storage(&storage).unwrap_or_default();
        let registry = shared_skill_registry(initial);
        let provider = Arc::new(LocalFilesystemSkillProvider);
        Self {
            storage,
            registry,
            provider,
            skills_dir,
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
        let source = req.source.trim();

        // 1. 判断是否是远程资源并获取真实的本地源路径
        let (src_path, _temp_dir_guard) = if source.starts_with("https://")
            || source.starts_with("http://")
            || source.starts_with("github:")
        {
            // 解析类似 github:username/repo 为真实 URL
            let url = if let Some(repo) = source.strip_prefix("github:") {
                format!("https://github.com/{}", repo)
            } else {
                source.to_string()
            };

            // 创建临时目录用于 clone
            let temp_dir = tempfile::TempDir::new().map_err(|e| {
                SkillServiceError::InvalidSource(format!("Failed to create temp dir: {e}"))
            })?;

            // 调用系统 Git 进行 clone
            let status = std::process::Command::new("git")
                .args([
                    "clone",
                    "--depth",
                    "1",
                    &url,
                    temp_dir.path().to_str().unwrap(),
                ])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map_err(|e| SkillServiceError::InvalidSource(format!("Git clone failed: {e}")))?;

            if !status.success() {
                return Err(SkillServiceError::InvalidSource(format!(
                    "Git clone failed for URL: {url}"
                )));
            }

            // 返回临时目录路径，同时返回 guard 防止目录在解析完成前被销毁
            (temp_dir.path().to_path_buf(), Some(temp_dir))
        } else {
            // 本地路径
            (PathBuf::from(source), None)
        };

        // 2. 检查源路径有效性
        if !src_path.is_dir() {
            return Err(SkillServiceError::InvalidSource(source.to_string()));
        }

        let real_root = find_skill_root(&src_path).ok_or_else(|| {
            SkillServiceError::InvalidSource(format!("Cannot find SKILL.md in {source}"))
        })?;

        let skill_md_content = fs::read_to_string(real_root.join("SKILL.md"))
            .map_err(|e| SkillServiceError::InvalidSource(e.to_string()))?;

        // 3. 确定最终名称
        let folder_name = real_root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "custom-skill".into());

        let (parsed_name, description, version) =
            parse_skill_markdown(&skill_md_content, &folder_name);
        let skill_name = req.name_override.unwrap_or(parsed_name).trim().to_string();

        if let Ok(Some(_)) = self.storage.get_skill_by_name(&skill_name) {
            return Err(SkillServiceError::AlreadyInstalled(skill_name));
        }

        // 4. 执行物理拷贝：将有效内容从源（或临时 Clone 目录）拷贝到真实技能库
        let target_dir = self.skills_dir.join(&skill_name);
        if !target_dir.exists() {
            copy_dir_all(&real_root, &target_dir)?;
        }

        // 5. 元数据落库与刷新
        let id = uuid::Uuid::new_v4().to_string();
        let record = StoredSkillRecord {
            id: id.clone(),
            name: skill_name,
            description,
            version,
            source: req.source.to_string(), // 保留用户输入的原始来源
            path: target_dir,
            enabled: true,
            installed_at: String::new(),
            updated_at: String::new(),
        };

        self.storage.upsert_skill(record)?;
        reload_skill_registry(&self.registry, &self.storage)?;

        let saved = self
            .storage
            .load_skills()?
            .into_iter()
            .find(|s| s.id == id)
            .unwrap();

        // 函数结束时，若存在 _temp_dir_guard，它的 Drop 逻辑会自动删除克隆下来的临时文件，绝不污染系统！
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

    pub fn resolve_candidates(
        &self,
        project_name: Option<&str>,
        query: &str,
    ) -> Vec<SkillMetadata> {
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
