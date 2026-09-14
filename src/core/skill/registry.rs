// src/core/skill/registry.rs
//! In-memory view of installed Skills and synchronization with Storage.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::core::skill::types::SkillMetadata;
use crate::storage::Storage;

#[derive(Debug, Clone, Default)]
pub struct SkillRegistry {
    skills: HashMap<String, SkillMetadata>,
    by_name: HashMap<String, String>,
}

impl SkillRegistry {
    pub fn from_storage(storage: &Storage) -> anyhow::Result<Self> {
        let records = storage.load_skills()?;
        let mut skills = HashMap::new();
        let mut by_name = HashMap::new();
        for r in records {
            let meta = SkillMetadata {
                id: r.id.clone(),
                name: r.name.clone(),
                description: r.description,
                version: r.version,
                source: r.source,
                path: r.path,
                enabled: r.enabled,
            };
            by_name.insert(r.name.to_ascii_lowercase(), r.id.clone());
            skills.insert(r.id, meta);
        }
        Ok(Self { skills, by_name })
    }

    pub fn get_by_name(&self, name: &str) -> Option<&SkillMetadata> {
        let key = name.trim().to_ascii_lowercase();
        self.by_name.get(&key).and_then(|id| self.skills.get(id))
    }

    pub fn get_by_id(&self, id: &str) -> Option<&SkillMetadata> {
        self.skills.get(id)
    }

    pub fn list(&self) -> Vec<&SkillMetadata> {
        let mut list: Vec<&SkillMetadata> = self.skills.values().collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }
}

pub type SharedSkillRegistry = Arc<RwLock<Arc<SkillRegistry>>>;

pub fn shared_skill_registry(registry: SkillRegistry) -> SharedSkillRegistry {
    Arc::new(RwLock::new(Arc::new(registry)))
}

pub fn reload_skill_registry(
    shared: &SharedSkillRegistry,
    storage: &Storage,
) -> anyhow::Result<()> {
    let fresh = SkillRegistry::from_storage(storage)?;
    if let Ok(mut lock) = shared.write() {
        *lock = Arc::new(fresh);
    }
    Ok(())
}
