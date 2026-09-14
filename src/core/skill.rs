// src/core/skill.rs
//! Skill Domain: Standard SKILL.md discovery, lifecycle, registry, and resolution.

pub mod provider;
pub mod registry;
pub mod resolver;
pub mod service;
pub mod types;

pub use provider::{LocalFilesystemSkillProvider, SkillProvider, SkillProviderError};
pub use registry::{reload_skill_registry, shared_skill_registry, SharedSkillRegistry, SkillRegistry};
pub use resolver::SkillResolver;
pub use service::{SkillService, SkillServiceError};
pub use types::{default_skills_dir, find_skill_root, parse_skill_markdown, SkillContent, SkillMetadata};