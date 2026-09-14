// src/core/skill.rs
//! Skill Domain: Standard SKILL.md discovery, lifecycle, registry, and resolution.

pub mod agent;
pub mod provider;
pub mod registry;
pub mod resolver;
pub mod service;
pub mod types;

pub use agent::{
    find_agent_dir, load_agent_view, AgentManifest, AgentModelError, AgentRule, AgentView,
};
pub use provider::{LocalFilesystemSkillProvider, SkillProvider, SkillProviderError};
pub use registry::{
    reload_skill_registry, shared_skill_registry, SharedSkillRegistry, SkillRegistry,
};
pub use resolver::SkillResolver;
pub use service::{SkillService, SkillServiceError};
pub use types::{find_skill_root, parse_skill_markdown, SkillContent, SkillMetadata};
