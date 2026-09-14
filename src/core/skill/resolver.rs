// src/core/skill/resolver.rs
//! Deterministic Skill Resolver providing candidates for Brain selection.

use crate::core::skill::types::SkillMetadata;
use std::collections::HashSet;

pub struct SkillResolver;

impl SkillResolver {
    /// Resolve candidate skills based on query keywords matching name or description.
    pub fn resolve_candidates<'a>(
        available_skills: &'a [&SkillMetadata],
        query: &str,
    ) -> Vec<&'a SkillMetadata> {
        let stop_words = ["with", "that", "this", "from", "then"];

        let terms: Vec<String> = query
            .split_whitespace()
            .map(|s| s.trim().to_ascii_lowercase())
            // 过滤短词和常见停用词，大幅降低误命中率
            .filter(|s| s.len() > 3 && !stop_words.contains(&s.as_str()))
            .collect();

        if terms.is_empty() {
            return available_skills.to_vec();
        }

        let mut matched = Vec::new();
        let mut seen = HashSet::new();

        for skill in available_skills {
            if !skill.enabled {
                continue;
            }
            let name_lower = skill.name.to_ascii_lowercase();
            let desc_lower = skill.description.to_ascii_lowercase();

            let matches = terms
                .iter()
                .any(|term| name_lower.contains(term) || desc_lower.contains(term));

            if matches && seen.insert(&skill.id) {
                matched.push(*skill);
            }
        }

        matched
    }
}
