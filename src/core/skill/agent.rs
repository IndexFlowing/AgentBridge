// src/core/skill/agent.rs
//! The `.agent` project model: `agent.yaml`, `rules/`, and `skills/`.
//!
//! Responsibilities are kept strictly separate:
//! - `agent.yaml` holds agent/server entry metadata and configuration only.
//! - `rules/` holds global rule documents merged into skill execution context.
//! - `skills/` holds skill definitions, compatible with the Agent Skills standard
//!   `SKILL.md` and with the lightweight `skill.yaml` + `system.md` layout.
//!
//! Rules are never mixed into skills.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::core::skill::types::{parse_skill_markdown, SkillMetadata};

pub const AGENT_DIR_NAME: &str = ".agent";
pub const MANIFEST_FILE: &str = "agent.yaml";
pub const RULES_DIR_NAME: &str = "rules";
pub const SKILLS_DIR_NAME: &str = "skills";
pub const PROJECTS_DIR_NAME: &str = "projects";
pub const SKILL_MANIFEST_FILE: &str = "skill.yaml";
pub const SKILL_PROMPT_FILE: &str = "system.md";
pub const AGENT_SKILL_SOURCE: &str = "agent";

#[derive(Debug, Error)]
pub enum AgentModelError {
    #[error("agent manifest not found: {0}")]
    ManifestNotFound(String),
    #[error("rule '{0}' escapes the .agent directory")]
    RuleOutsideAgent(String),
    #[error("I/O error reading .agent: {0}")]
    Io(#[from] std::io::Error),
}

/// Entry-level metadata and configuration declared by `.agent/agent.yaml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentManifest {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Rule documents (relative to `.agent/`) merged into skill execution.
    #[serde(default)]
    pub global_rules: Vec<String>,
    /// Default enabled skill set declared under `.agent/skills/`.
    #[serde(default)]
    pub active_skills: Vec<String>,
    /// Skill paths declared under `skills.load` (relative to `.agent/`). These
    /// point at a skill folder or its `SKILL.md`/`skill.yaml`.
    #[serde(default)]
    pub skills_load: Vec<String>,
}

/// A single rule document loaded from `.agent/rules/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRule {
    pub name: String,
    pub path: String,
    pub content: String,
}

/// Resolved view of a project's `.agent` directory.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentView {
    pub found: bool,
    pub agent_dir: String,
    pub manifest: Option<AgentManifest>,
    pub rules: Vec<AgentRule>,
    pub skills: Vec<SkillMetadata>,
}

/// Resolve a child entry of `parent` by `name`, matching case-insensitively.
///
/// The canonical Agent layout uses lowercase names (`agent.yaml`, `rules/`,
/// `skills/`, `projects/`), but an installation authored on a case-insensitive
/// filesystem may carry `Agent.yaml`, `Rules/`, `Skills/`. On case-sensitive
/// filesystems those names differ, so an exact miss falls back to a directory
/// scan with ASCII case folding. This keeps both spellings working on
/// Linux/macOS without hard-coding a second set of names.
fn resolve_entry_ci(parent: &Path, name: &str) -> Option<PathBuf> {
    let exact = parent.join(name);
    if exact.is_file() || exact.is_dir() {
        return Some(exact);
    }
    let entries = fs::read_dir(parent).ok()?;
    for entry in entries.flatten() {
        if entry
            .file_name()
            .to_string_lossy()
            .eq_ignore_ascii_case(name)
        {
            return Some(entry.path());
        }
    }
    None
}

/// Locate a `.agent` directory directly under `root`.
pub fn find_agent_dir(root: &Path) -> Option<PathBuf> {
    resolve_entry_ci(root, AGENT_DIR_NAME).filter(|dir| dir.is_dir())
}

/// Load and resolve the complete `.agent` view for a project root.
///
/// A missing `.agent` directory is not an error: an empty view is returned so
/// that existing projects keep working without any migration.
pub fn load_agent_view(root: &Path) -> Result<AgentView, AgentModelError> {
    let Some(agent_dir) = find_agent_dir(root) else {
        return Ok(AgentView::default());
    };
    load_agent_dir_view(&agent_dir)
}

/// Load and resolve an `.agent` directory directly.
///
/// A missing directory or manifest is not an error: the view degrades to an
/// empty (or manifest-less) view so AgentBridge can run before any Agent
/// configuration has been authored.
pub fn load_agent_dir_view(agent_dir: &Path) -> Result<AgentView, AgentModelError> {
    if !agent_dir.is_dir() {
        return Ok(AgentView::default());
    }

    let manifest = match load_manifest(agent_dir) {
        Ok(manifest) => manifest,
        Err(AgentModelError::ManifestNotFound(_)) => AgentManifest::default(),
        Err(err) => return Err(err),
    };
    let rules = load_rules(agent_dir, &manifest)?;
    let skills = discover_skills(agent_dir, &manifest)?;

    Ok(AgentView {
        found: true,
        agent_dir: agent_dir.display().to_string(),
        manifest: Some(manifest),
        rules,
        skills,
    })
}

/// Parse `.agent/agent.yaml` into an [`AgentManifest`].
pub fn load_manifest(agent_dir: &Path) -> Result<AgentManifest, AgentModelError> {
    let Some(path) = resolve_entry_ci(agent_dir, MANIFEST_FILE).filter(|p| p.is_file()) else {
        return Err(AgentModelError::ManifestNotFound(
            agent_dir.join(MANIFEST_FILE).display().to_string(),
        ));
    };
    let text = fs::read_to_string(&path)?;
    Ok(parse_agent_manifest(&text))
}

/// Parse the supported YAML subset of `agent.yaml`.
///
/// Both a flat layout (`name:`, `global_rules:`) and the richer installed
/// layout (`agent: { name: ... }` with `rules: { load: [...] }`) are accepted.
/// Top-level values always win; nested values are used as fallbacks so the
/// shipped Agent root is observable without rewriting user configuration.
pub fn parse_agent_manifest(text: &str) -> AgentManifest {
    let (scalars, lists) = parse_yaml_subset(text);
    let (nested_scalars, nested_lists) = parse_nested_yaml(text);

    let mut manifest = AgentManifest {
        version: scalars.get("version").cloned().unwrap_or_default(),
        name: scalars.get("name").cloned().unwrap_or_default(),
        description: scalars.get("description").cloned().unwrap_or_default(),
        global_rules: lists.get("global_rules").cloned().unwrap_or_default(),
        active_skills: lists.get("active_skills").cloned().unwrap_or_default(),
        skills_load: lists.get("skills_load").cloned().unwrap_or_default(),
    };

    if manifest.name.trim().is_empty() {
        if let Some(name) = nested_scalars.get("agent.name") {
            manifest.name = name.clone();
        }
    }
    if manifest.description.trim().is_empty() {
        if let Some(description) = nested_scalars.get("agent.description") {
            manifest.description = description.clone();
        }
    }
    if manifest.global_rules.is_empty() {
        if let Some(rules) = nested_lists.get("rules.load") {
            manifest.global_rules = rules.clone();
        }
    }
    if manifest.skills_load.is_empty() {
        if let Some(skills) = nested_lists.get("skills.load") {
            manifest.skills_load = skills.clone();
        }
    }

    manifest
}

/// Nested YAML reader for `section.key` scalars and `section.key` string lists.
/// Only one level of nesting is supported, which is all the Agent root needs.
fn parse_nested_yaml(text: &str) -> (BTreeMap<String, String>, BTreeMap<String, Vec<String>>) {
    let mut scalars = BTreeMap::new();
    let mut lists: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut section = String::new();
    let mut child: Option<String> = None;

    for raw in text.lines() {
        let line = raw.trim_end();
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("```") {
            continue;
        }
        let indent = line.len() - line.trim_start().len();

        if let Some(item) = trimmed.strip_prefix('-') {
            if indent > 0 {
                if let Some(key) = &child {
                    let value = unquote(strip_inline_comment(item).trim());
                    if !value.is_empty() {
                        lists
                            .entry(format!("{section}.{key}"))
                            .or_default()
                            .push(value);
                    }
                }
            }
            continue;
        }

        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        let key = key.trim().to_string();
        let value = strip_inline_comment(value).trim();

        if indent == 0 {
            section = key;
            child = None;
            continue;
        }
        if section.is_empty() {
            continue;
        }
        if value.is_empty() || value == ">" || value == "|" {
            child = Some(key.clone());
            lists.entry(format!("{section}.{key}")).or_default();
        } else {
            scalars.insert(format!("{section}.{key}"), unquote(value));
            child = None;
        }
    }

    (scalars, lists)
}

/// Load rule documents declared in `global_rules`, then any extra `*.md`
/// present in `.agent/rules/`. Rules stay separate from skills.
pub fn load_rules(
    agent_dir: &Path,
    manifest: &AgentManifest,
) -> Result<Vec<AgentRule>, AgentModelError> {
    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for declared in &manifest.global_rules {
        let rel = declared.trim();
        if rel.is_empty() {
            continue;
        }
        let resolved = resolve_within(agent_dir, rel)
            .ok_or_else(|| AgentModelError::RuleOutsideAgent(rel.to_string()))?;
        if resolved.is_file() {
            push_rule(&mut out, &mut seen, agent_dir, &resolved)?;
        }
    }

    let rules_dir = resolve_entry_ci(agent_dir, RULES_DIR_NAME);
    if let Some(rules_dir) = rules_dir.filter(|dir| dir.is_dir()) {
        let mut entries: Vec<_> = fs::read_dir(&rules_dir)?.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "md") {
                push_rule(&mut out, &mut seen, agent_dir, &path)?;
            }
        }
    }

    Ok(out)
}

/// Discover skills under `.agent/skills/` and mark them enabled according to
/// `active_skills` and `skills.load`.
///
/// Declared skills are authoritative: when either list is non-empty, only the
/// declared skills are enabled. When both are empty, every discovered skill is
/// enabled. `skills.load` entries are also followed as discovery roots, so a
/// manifest can point at a skill folder (or its `SKILL.md`) that is not a
/// direct child of `.agent/skills/`.
pub fn discover_skills(
    agent_dir: &Path,
    manifest: &AgentManifest,
) -> Result<Vec<SkillMetadata>, AgentModelError> {
    let mut dirs: Vec<PathBuf> = Vec::new();

    let skills_root = resolve_entry_ci(agent_dir, SKILLS_DIR_NAME);
    if let Some(skills_root) = skills_root.filter(|dir| dir.is_dir()) {
        let mut entries: Vec<_> = fs::read_dir(&skills_root)?.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let dir = entry.path();
            if dir.is_dir() {
                dirs.push(dir);
            }
        }
    }

    for declared in &manifest.skills_load {
        if let Some(dir) = declared_skill_dir(agent_dir, declared) {
            dirs.push(dir);
        }
    }

    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for dir in dirs {
        let key = fs::canonicalize(&dir)
            .unwrap_or_else(|_| dir.clone())
            .to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase();
        if !seen.insert(key) {
            continue;
        }
        if let Some(skill) = build_skill_metadata(&dir, manifest) {
            out.push(skill);
        }
    }
    Ok(out)
}

fn build_skill_metadata(dir: &Path, manifest: &AgentManifest) -> Option<SkillMetadata> {
    let (name, description, version) = parse_agent_skill(dir)?;
    let enabled = skill_is_enabled(dir, &name, manifest);
    Some(SkillMetadata {
        id: format!("{AGENT_SKILL_SOURCE}:{name}"),
        name,
        description,
        version,
        source: AGENT_SKILL_SOURCE.to_string(),
        path: dir.to_path_buf(),
        enabled,
    })
}

fn skill_is_enabled(dir: &Path, name: &str, manifest: &AgentManifest) -> bool {
    if manifest.active_skills.is_empty() && manifest.skills_load.is_empty() {
        return true;
    }
    if is_active(name, &manifest.active_skills) {
        return true;
    }
    let folder = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    manifest.skills_load.iter().any(|declared| {
        declared_skill_token(declared)
            .is_some_and(|t| t.eq_ignore_ascii_case(&folder) || t.eq_ignore_ascii_case(name))
    })
}

/// Resolve a declared `skills.load` entry to the skill directory it points at.
/// `None` when the entry is empty, escapes `.agent/`, or does not exist.
fn declared_skill_dir(agent_dir: &Path, declared: &str) -> Option<PathBuf> {
    let rel = declared.trim();
    if rel.is_empty() {
        return None;
    }
    let path = resolve_within(agent_dir, rel)?;
    if path.is_dir() {
        return Some(path);
    }
    if path.is_file() {
        return path.parent().map(Path::to_path_buf);
    }
    None
}

/// Extract the skill token a `skills.load` entry refers to. File entries such
/// as `Skills/rust-skills/SKILL.md` yield their containing folder name.
fn declared_skill_token(declared: &str) -> Option<String> {
    let normalized = declared.trim().replace('\\', "/");
    if normalized.is_empty() {
        return None;
    }
    let path = Path::new(&normalized);
    let token = match path.extension().and_then(|e| e.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("yaml") => {
            path.parent().and_then(|p| p.file_name())
        }
        _ => path.file_name(),
    }?;
    let token = token.to_string_lossy().into_owned();
    (!token.trim().is_empty()).then_some(token)
}

/// Parse a single skill folder: standard `SKILL.md` first, then `skill.yaml`.
pub fn parse_agent_skill(dir: &Path) -> Option<(String, String, String)> {
    let folder = dir.file_name()?.to_string_lossy().into_owned();

    let skill_md = resolve_entry_ci(dir, "SKILL.md");
    if let Some(skill_md) = skill_md.filter(|p| p.is_file()) {
        let content = fs::read_to_string(&skill_md).ok()?;
        return Some(parse_skill_markdown(&content, &folder));
    }

    let manifest = resolve_entry_ci(dir, SKILL_MANIFEST_FILE);
    if let Some(manifest) = manifest.filter(|p| p.is_file()) {
        let text = fs::read_to_string(&manifest).ok()?;
        let (scalars, _) = parse_yaml_subset(&text);
        let name = scalars.get("name").cloned().unwrap_or(folder);
        let description = scalars.get("description").cloned().unwrap_or_default();
        let version = scalars
            .get("version")
            .cloned()
            .unwrap_or_else(|| "1.0.0".to_string());
        return Some((name, description, version));
    }

    None
}

fn is_active(name: &str, active_skills: &[String]) -> bool {
    let name = name.trim();
    active_skills
        .iter()
        .any(|s| s.trim().eq_ignore_ascii_case(name))
}

fn push_rule(
    out: &mut Vec<AgentRule>,
    seen: &mut HashSet<String>,
    agent_dir: &Path,
    path: &Path,
) -> Result<(), AgentModelError> {
    let key = fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string();
    if !seen.insert(key) {
        return Ok(());
    }
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    let rel = relative_rule_path(agent_dir, path);
    out.push(AgentRule {
        name,
        path: rel,
        content: fs::read_to_string(path)?,
    });
    Ok(())
}

/// Portable, separator-normalized rule path relative to the Agent root. Used
/// as the dedup key so `/` and `\` spellings of the same rule collapse.
fn relative_rule_path(agent_dir: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(agent_dir).unwrap_or(path);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Resolve a declared path while rejecting absolute, `~`, and parent
/// traversals so it can never escape `.agent/`.
///
/// Each component is matched case-insensitively against the filesystem when it
/// exists, so a declaration such as `rules/base.md` still resolves when the
/// on-disk directory is `Rules/`. Missing components are joined literally so
/// callers keep their existing existence checks.
fn resolve_within(base: &Path, rel: &str) -> Option<PathBuf> {
    if rel.starts_with('~') {
        return None;
    }
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() {
        return None;
    }
    if rel_path
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return None;
    }

    let mut resolved = base.to_path_buf();
    for component in rel_path.components() {
        match component {
            Component::Normal(part) => {
                let name = part.to_string_lossy();
                resolved =
                    resolve_entry_ci(&resolved, &name).unwrap_or_else(|| resolved.join(part));
            }
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(resolved)
}

/// Minimal YAML subset reader: top-level scalars and top-level block/inline
/// string lists. Nested maps and nested lists are intentionally ignored.
fn parse_yaml_subset(text: &str) -> (BTreeMap<String, String>, BTreeMap<String, Vec<String>>) {
    let mut scalars = BTreeMap::new();
    let mut lists: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut current_list: Option<String> = None;

    for raw in text.lines() {
        let line = raw.trim_end();
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.len() - line.trim_start().len();

        if let Some(item) = trimmed.strip_prefix('-') {
            if let Some(key) = &current_list {
                let value = unquote(strip_inline_comment(item).trim());
                if !value.is_empty() {
                    lists.entry(key.clone()).or_default().push(value);
                }
            }
            continue;
        }

        if indent == 0 {
            current_list = None;
            if let Some((key, value)) = trimmed.split_once(':') {
                let key = key.trim().to_string();
                let value = strip_inline_comment(value).trim();
                if value.is_empty() {
                    current_list = Some(key.clone());
                    lists.entry(key).or_default();
                } else if value.starts_with('[') {
                    let items = parse_inline_list(value);
                    if !items.is_empty() {
                        lists.insert(key, items);
                    }
                } else {
                    scalars.insert(key, unquote(value));
                }
            }
        } else if trimmed.contains(':') {
            // Entering a nested map/list: stop collecting the parent list.
            current_list = None;
        }
    }

    (scalars, lists)
}

fn parse_inline_list(value: &str) -> Vec<String> {
    let inner = value.trim().trim_start_matches('[').trim_end_matches(']');
    inner
        .split(',')
        .map(|s| unquote(strip_inline_comment(s).trim()))
        .filter(|s| !s.is_empty())
        .collect()
}

fn strip_inline_comment(value: &str) -> &str {
    let bytes = value.as_bytes();
    let mut quote: Option<u8> = None;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                }
            }
            None => {
                if c == b'"' || c == b'\'' {
                    quote = Some(c);
                } else if c == b'#' && (i == 0 || bytes[i - 1].is_ascii_whitespace()) {
                    return &value[..i];
                }
            }
        }
        i += 1;
    }
    value
}

fn unquote(value: &str) -> String {
    let v = value.trim();
    let bytes = v.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        return v[1..v.len() - 1].to_string();
    }
    v.to_string()
}

/// AgentBridge-managed project binding profile (`projects/*.yaml`).
///
/// A profile binds an AgentBridge project name to a workspace and to the
/// rules/skills drawn from the global Agent root. It is *not* a project-owned
/// `.agent` directory and never duplicates long-term Agent configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectProfile {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub workspace: String,
    #[serde(default)]
    pub rules: Vec<String>,
    #[serde(default)]
    pub skills: Vec<String>,
}

/// Parse the supported YAML subset of a `projects/*.yaml` profile.
pub fn parse_project_profile(text: &str) -> ProjectProfile {
    let (scalars, lists) = parse_yaml_subset(text);
    ProjectProfile {
        name: scalars.get("name").cloned().unwrap_or_default(),
        workspace: scalars.get("workspace").cloned().unwrap_or_default(),
        rules: lists.get("rules").cloned().unwrap_or_default(),
        skills: lists.get("skills").cloned().unwrap_or_default(),
    }
}

/// Load `projects/<name>.yaml`, falling back to scanning the profile directory
/// for a matching top-level `name:` field.
pub fn load_project_profile(agent_dir: &Path, project_name: &str) -> Option<ProjectProfile> {
    let dir = resolve_entry_ci(agent_dir, PROJECTS_DIR_NAME).filter(|dir| dir.is_dir())?;

    let direct = dir.join(format!("{project_name}.yaml"));
    if direct.is_file() {
        let text = fs::read_to_string(&direct).ok()?;
        return Some(with_profile_name(
            parse_project_profile(&text),
            project_name,
        ));
    }

    let mut entries: Vec<_> = fs::read_dir(&dir).ok()?.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        if !is_yaml(&path) {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let profile = parse_project_profile(&text);
        if profile.name.eq_ignore_ascii_case(project_name) {
            return Some(with_profile_name(profile, project_name));
        }
    }
    None
}

fn with_profile_name(mut profile: ProjectProfile, fallback: &str) -> ProjectProfile {
    if profile.name.trim().is_empty() {
        profile.name = fallback.to_string();
    }
    profile
}

fn is_yaml(path: &Path) -> bool {
    path.is_file()
        && matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("yaml") | Some("yml")
        )
}

/// Fully resolved per-task context handed to the C2C/Executor boundary.
///
/// Assembled from the AgentBridge Agent root (`agent.yaml` + `rules/`), the
/// AgentBridge project profile, and the requested skills. Executors receive
/// this object; they never read a project-owned `.agent` directory.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentContext {
    pub task_id: String,
    pub project: String,
    pub workspace: String,
    pub agent_root: String,
    pub manifest: Option<AgentManifest>,
    pub rules: Vec<AgentRule>,
    pub skills: Vec<String>,
}

impl AgentContext {
    /// Render the context as a compact block appended to the executor prompt.
    /// Skill bodies are intentionally excluded (skill-model agnostic).
    pub fn render(&self) -> String {
        let mut out = String::from("\nAGENT_CONTEXT:\n");
        out.push_str(&format!("PROJECT: {}\n", self.project));
        out.push_str(&format!("WORKSPACE: {}\n", self.workspace));
        out.push_str(&format!("AGENT_ROOT: {}\n", self.agent_root));
        if let Some(manifest) = &self.manifest {
            if !manifest.name.trim().is_empty() {
                out.push_str(&format!("AGENT_NAME: {}\n", manifest.name));
            }
        }
        if !self.skills.is_empty() {
            out.push_str("CONTEXT_SKILLS:\n");
            for skill in &self.skills {
                out.push_str(&format!("- {skill}\n"));
            }
        }
        if !self.rules.is_empty() {
            out.push_str("CONTEXT_RULES:\n");
            for rule in &self.rules {
                out.push_str(&format!("--- {} ---\n", rule.path));
                out.push_str(rule.content.trim_end());
                out.push('\n');
            }
        }
        out
    }
}

/// Resolve a single task's [`AgentContext`] from the Agent root.
///
/// This is a pure resolution step: it reads only AgentBridge-managed data and
/// never treats a workspace `.agent` directory as authoritative.
pub fn resolve_agent_context(
    agent_dir: &Path,
    project_name: &str,
    workspace: &Path,
    task_id: &str,
    requested_skills: &[String],
) -> Result<AgentContext, AgentModelError> {
    let view = load_agent_dir_view(agent_dir)?;
    let profile = load_project_profile(agent_dir, project_name);

    let mut seen_rules: HashSet<String> = HashSet::new();
    let mut rules = Vec::new();
    for rule in &view.rules {
        if seen_rules.insert(rule.path.clone()) {
            rules.push(rule.clone());
        }
    }
    if let Some(profile) = &profile {
        for declared in &profile.rules {
            let rel = declared.trim();
            if rel.is_empty() {
                continue;
            }
            let Some(path) = resolve_within(agent_dir, rel) else {
                continue;
            };
            if !path.is_file() {
                continue;
            }
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            let rel_path = relative_rule_path(agent_dir, &path);
            if seen_rules.insert(rel_path.clone()) {
                rules.push(AgentRule {
                    name,
                    path: rel_path,
                    content: fs::read_to_string(&path)?,
                });
            }
        }
    }

    // Enabled skills discovered under the Agent root form the baseline layer of
    // the context; explicit requests, project profiles, and `active_skills`
    // extend it. Disabled skills are intentionally excluded.
    let discovered_enabled = view.skills.iter().filter(|s| s.enabled).map(|s| &s.name);

    let mut skills: Vec<String> = Vec::new();
    for skill in requested_skills
        .iter()
        .chain(profile.iter().flat_map(|p| p.skills.iter()))
        .chain(view.manifest.iter().flat_map(|m| m.active_skills.iter()))
        .chain(discovered_enabled)
    {
        let skill = skill.trim();
        if skill.is_empty() {
            continue;
        }
        if !skills.iter().any(|s| s.eq_ignore_ascii_case(skill)) {
            skills.push(skill.to_string());
        }
    }

    let workspace = profile
        .as_ref()
        .map(|p| p.workspace.trim())
        .filter(|w| !w.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| workspace.display().to_string());

    Ok(AgentContext {
        task_id: task_id.to_string(),
        project: project_name.to_string(),
        workspace,
        agent_root: agent_dir.display().to_string(),
        manifest: view.manifest,
        rules,
        skills,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[test]
    fn parses_manifest_scalars_and_lists() {
        let text = r#"
version: "1.0.0"
name: "Demo Agent"
description: "Context-aware assistant for X"  # trailing comment
global_rules:
  - "rules/alpha.md"
  - rules/beta.md
active_skills: [one, "two"]
"#;
        let m = parse_agent_manifest(text);
        assert_eq!(m.version, "1.0.0");
        assert_eq!(m.name, "Demo Agent");
        assert_eq!(m.description, "Context-aware assistant for X");
        assert_eq!(m.global_rules, vec!["rules/alpha.md", "rules/beta.md"]);
        assert_eq!(m.active_skills, vec!["one", "two"]);
    }

    #[test]
    fn parses_nested_installed_agent_manifest() {
        let text = "```yaml\nversion: \"1\"\n\nagent:\n  id: \"agentbridge\"\n  name: \"AgentBridge Brain\"\n  description: >\n    layered agent\n\nrules:\n  load:\n    - Rules/core.md\n    - Rules/architecture.md\nskills:\n  load:\n    - Skills/rust-skills/SKILL.md\n    - Skills/design-pattern-review/SKILL.md\n";
        let m = parse_agent_manifest(text);
        assert_eq!(m.version, "1");
        assert_eq!(m.name, "AgentBridge Brain");
        assert_eq!(
            m.global_rules,
            vec!["Rules/core.md", "Rules/architecture.md"]
        );
        assert_eq!(
            m.skills_load,
            vec![
                "Skills/rust-skills/SKILL.md",
                "Skills/design-pattern-review/SKILL.md"
            ]
        );
    }

    #[test]
    fn skills_load_declares_enabled_skills() {
        let root = TempDir::new().unwrap();
        write(
            &root.path().join(".agent/agent.yaml"),
            "version: \"1\"\nname: \"Fixture\"\nskills:\n  load:\n    - skills/alpha/SKILL.md\n",
        );
        write(
            &root.path().join(".agent/skills/alpha/SKILL.md"),
            "---\nname: alpha\n---\n# Alpha\n",
        );
        write(
            &root.path().join(".agent/skills/beta/SKILL.md"),
            "---\nname: beta\n---\n# Beta\n",
        );

        let view = load_agent_dir_view(&root.path().join(".agent")).unwrap();
        let alpha = view.skills.iter().find(|s| s.name == "alpha").unwrap();
        let beta = view.skills.iter().find(|s| s.name == "beta").unwrap();
        assert!(alpha.enabled, "declared skill must be enabled");
        assert!(!beta.enabled, "undeclared skill must stay disabled");

        let context = resolve_agent_context(
            &root.path().join(".agent"),
            "demo",
            Path::new("."),
            "c2c_load",
            &[],
        )
        .unwrap();
        assert!(context.skills.iter().any(|s| s == "alpha"));
        assert!(!context.skills.iter().any(|s| s == "beta"));
    }

    #[test]
    fn ignores_nested_skill_yaml_maps() {
        let text = r#"
name: "interaction-decision"
version: "1.0.0"
description: "Decide action"
requirements:
  min_context_window: 8192
  recommended_models:
    - "deepseek-chat"
"#;
        let (scalars, _lists) = parse_yaml_subset(text);
        assert_eq!(scalars.get("name").unwrap(), "interaction-decision");
        assert!(!scalars.contains_key("recommended_models"));
    }

    #[test]
    fn loads_agent_view_with_rules_and_skills() {
        let root = TempDir::new().unwrap();
        write(
            &root.path().join(".agent/agent.yaml"),
            "version: \"1.0.0\"\nname: \"Demo\"\nglobal_rules:\n  - \"rules/base.md\"\nactive_skills:\n  - \"interaction-decision\"\n",
        );
        write(
            &root.path().join(".agent/rules/base.md"),
            "# Base rules\nAlways respond in JSON.\n",
        );
        write(
            &root.path().join(".agent/rules/extra.md"),
            "# Extra rules\n",
        );
        write(
            &root
                .path()
                .join(".agent/skills/interaction-decision/skill.yaml"),
            "name: \"interaction-decision\"\nversion: \"2.0.0\"\ndescription: \"Pick an action\"\n",
        );
        write(
            &root
                .path()
                .join(".agent/skills/interaction-decision/system.md"),
            "# Role\nDecide.\n",
        );
        write(
            &root.path().join(".agent/skills/standard/SKILL.md"),
            "---\nname: standard-skill\nversion: 0.9.0\ndescription: A standard skill\n---\n# Standard\n",
        );

        let view = load_agent_view(root.path()).unwrap();
        assert!(view.found);
        let manifest = view.manifest.unwrap();
        assert_eq!(manifest.name, "Demo");

        let rule_names: Vec<_> = view.rules.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(rule_names, vec!["base", "extra"]);
        assert!(view.rules[0].content.contains("JSON"));

        let skills: Vec<_> = view.skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(skills, vec!["interaction-decision", "standard-skill"]);

        let active = view
            .skills
            .iter()
            .find(|s| s.name == "interaction-decision")
            .unwrap();
        assert_eq!(active.version, "2.0.0");
        assert!(active.enabled);
        assert_eq!(active.source, "agent");

        let inactive = view
            .skills
            .iter()
            .find(|s| s.name == "standard-skill")
            .unwrap();
        assert!(!inactive.enabled);
        assert_eq!(inactive.version, "0.9.0");
    }

    #[test]
    fn rejects_rule_traversal() {
        let root = TempDir::new().unwrap();
        write(
            &root.path().join(".agent/agent.yaml"),
            "global_rules:\n  - \"../secret.md\"\n",
        );
        let err = load_agent_view(root.path()).unwrap_err();
        assert!(matches!(err, AgentModelError::RuleOutsideAgent(_)));
    }

    #[test]
    fn missing_agent_dir_is_empty_not_error() {
        let root = TempDir::new().unwrap();
        let view = load_agent_view(root.path()).unwrap();
        assert!(!view.found);
        assert!(view.skills.is_empty());
    }

    #[test]
    fn capitalized_manifest_and_dirs_resolve() {
        let root = TempDir::new().unwrap();
        write(
            &root.path().join(".agent/Agent.yaml"),
            "version: \"1\"\nname: \"Legacy\"\nrules:\n  load:\n    - rules/base.md\n",
        );
        write(&root.path().join(".agent/Rules/base.md"), "# Base\n");

        let view = load_agent_view(root.path()).unwrap();
        assert!(view.found);
        assert_eq!(view.manifest.unwrap().name, "Legacy");
        assert_eq!(view.rules.len(), 1);
        assert_eq!(view.rules[0].name, "base");
    }

    #[test]
    fn capitalized_rules_scan_is_case_insensitive() {
        let root = TempDir::new().unwrap();
        write(
            &root.path().join(".agent/agent.yaml"),
            "name: \"Fixture\"\n",
        );
        write(&root.path().join(".agent/Rules/extra.md"), "# Extra\n");

        let view = load_agent_view(root.path()).unwrap();
        assert_eq!(view.rules.len(), 1);
        assert_eq!(view.rules[0].name, "extra");
    }
}
