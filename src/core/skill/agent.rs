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

/// Locate a `.agent` directory directly under `root`.
pub fn find_agent_dir(root: &Path) -> Option<PathBuf> {
    let dir = root.join(AGENT_DIR_NAME);
    dir.is_dir().then_some(dir)
}

/// Load and resolve the complete `.agent` view for a project root.
///
/// A missing `.agent` directory is not an error: an empty view is returned so
/// that existing projects keep working without any migration.
pub fn load_agent_view(root: &Path) -> Result<AgentView, AgentModelError> {
    let Some(agent_dir) = find_agent_dir(root) else {
        return Ok(AgentView::default());
    };

    let manifest = load_manifest(&agent_dir)?;
    let rules = load_rules(&agent_dir, &manifest)?;
    let skills = discover_skills(&agent_dir, &manifest)?;

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
    let path = agent_dir.join(MANIFEST_FILE);
    if !path.is_file() {
        return Err(AgentModelError::ManifestNotFound(
            path.display().to_string(),
        ));
    }
    let text = fs::read_to_string(&path)?;
    Ok(parse_agent_manifest(&text))
}

/// Parse the supported YAML subset of `agent.yaml`.
pub fn parse_agent_manifest(text: &str) -> AgentManifest {
    let (scalars, lists) = parse_yaml_subset(text);
    AgentManifest {
        version: scalars.get("version").cloned().unwrap_or_default(),
        name: scalars.get("name").cloned().unwrap_or_default(),
        description: scalars.get("description").cloned().unwrap_or_default(),
        global_rules: lists.get("global_rules").cloned().unwrap_or_default(),
        active_skills: lists.get("active_skills").cloned().unwrap_or_default(),
    }
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

    let rules_dir = agent_dir.join(RULES_DIR_NAME);
    if rules_dir.is_dir() {
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
/// `active_skills`. An empty `active_skills` list enables every discovered skill.
pub fn discover_skills(
    agent_dir: &Path,
    manifest: &AgentManifest,
) -> Result<Vec<SkillMetadata>, AgentModelError> {
    let skills_root = agent_dir.join(SKILLS_DIR_NAME);
    if !skills_root.is_dir() {
        return Ok(Vec::new());
    }

    let mut entries: Vec<_> = fs::read_dir(&skills_root)?.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());

    let mut out = Vec::new();
    for entry in entries {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let Some((name, description, version)) = parse_agent_skill(&dir) else {
            continue;
        };
        let enabled =
            manifest.active_skills.is_empty() || is_active(&name, &manifest.active_skills);
        out.push(SkillMetadata {
            id: format!("{AGENT_SKILL_SOURCE}:{name}"),
            name,
            description,
            version,
            source: AGENT_SKILL_SOURCE.to_string(),
            path: dir,
            enabled,
        });
    }
    Ok(out)
}

/// Parse a single skill folder: standard `SKILL.md` first, then `skill.yaml`.
pub fn parse_agent_skill(dir: &Path) -> Option<(String, String, String)> {
    let folder = dir.file_name()?.to_string_lossy().into_owned();

    let skill_md = dir.join("SKILL.md");
    if skill_md.is_file() {
        let content = fs::read_to_string(&skill_md).ok()?;
        return Some(parse_skill_markdown(&content, &folder));
    }

    let manifest = dir.join(SKILL_MANIFEST_FILE);
    if manifest.is_file() {
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
    let rel = path
        .strip_prefix(agent_dir)
        .unwrap_or(path)
        .display()
        .to_string();
    out.push(AgentRule {
        name,
        path: rel,
        content: fs::read_to_string(path)?,
    });
    Ok(())
}

/// Resolve a declared rule path while rejecting absolute, `~`, and parent
/// traversals so rules can never escape `.agent/`.
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
    Some(base.join(rel_path))
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
}
