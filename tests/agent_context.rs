//! Architecture regression tests for the AgentBridge Agent Runtime migration:
//! the Agent root is authoritative, project `.agent` is not, a resolved
//! `AgentContext` is produced per task, and task state never relies on a
//! workspace `current.c2c` file.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use agentbridge::config::Config;
use agentbridge::core::skill::{
    load_agent_dir_view, load_project_profile, resolve_agent_context, SkillService,
};
use agentbridge::task::TaskService;
use tempfile::TempDir;

mod common;

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

#[test]
fn agent_root_lives_under_agentbridge_home() {
    let root = agentbridge::config::agent_root().unwrap();
    assert!(root.ends_with(Path::new(".agentbridge").join(".agent")));
}

#[test]
fn agent_root_is_authoritative_and_project_agent_is_ignored() {
    let agent_root = TempDir::new().unwrap();
    write(
        &agent_root.path().join("agent.yaml"),
        "version: \"1.0.0\"\nname: \"GlobalAgent\"\nglobal_rules:\n  - \"rules/global.md\"\n",
    );
    write(&agent_root.path().join("rules/global.md"), "# global\n");

    // A workspace that still carries a legacy project-owned `.agent`.
    let workspace = TempDir::new().unwrap();
    write(
        &workspace.path().join(".agent/agent.yaml"),
        "name: \"ProjectLocal\"\nglobal_rules:\n  - \"rules/local.md\"\n",
    );
    write(&workspace.path().join(".agent/rules/local.md"), "# local\n");

    let storage = common::test_storage();
    let service = SkillService::new(storage, agent_root.path().join("skills"))
        .with_agent_root(agent_root.path().to_path_buf());

    let view = service.agent_view(None).unwrap();
    assert!(view.found);
    assert_eq!(view.manifest.unwrap().name, "GlobalAgent");
    assert_eq!(view.rules.len(), 1);
    assert_eq!(view.rules[0].name, "global");
}

#[test]
fn project_profile_binds_workspace_rules_and_skills() {
    let agent_root = TempDir::new().unwrap();
    write(
        &agent_root.path().join("agent.yaml"),
        "name: \"GlobalAgent\"\nactive_skills:\n  - manifest-skill\n",
    );
    write(&agent_root.path().join("rules/global.md"), "# global\n");
    write(&agent_root.path().join("rules/project.md"), "# project\n");
    write(
        &agent_root.path().join("projects/alpha.yaml"),
        "name: alpha\nworkspace: /bound/workspace\nrules:\n  - rules/project.md\nskills:\n  - profile-skill\n",
    );

    let profile = load_project_profile(agent_root.path(), "alpha").expect("profile");
    assert_eq!(profile.workspace, "/bound/workspace");

    let workspace = TempDir::new().unwrap();
    let context = resolve_agent_context(
        agent_root.path(),
        "alpha",
        workspace.path(),
        "c2c_test_1",
        &["requested-skill".to_string()],
    )
    .unwrap();

    assert_eq!(context.workspace, "/bound/workspace");
    assert_eq!(context.task_id, "c2c_test_1");
    assert_eq!(context.project, "alpha");
    assert_eq!(context.rules.len(), 2);
    for expected in ["manifest-skill", "profile-skill", "requested-skill"] {
        assert!(
            context.skills.iter().any(|s| s == expected),
            "missing {expected}"
        );
    }

    let rendered = context.render();
    assert!(rendered.contains("AGENT_CONTEXT:"));
    assert!(rendered.contains("CONTEXT_SKILLS:"));
    assert!(rendered.contains("--- rules/project.md ---"));
}

#[test]
fn capitalized_layout_resolves_on_case_sensitive_filesystems() {
    let root = TempDir::new().unwrap();
    let agent = root.path();
    // Legacy/capitalized layout as authored or installed on some hosts.
    write(
        &agent.join("Agent.yaml"),
        "version: \"1\"\nname: \"Legacy\"\nrules:\n  load:\n    - rules/base.md\nskills:\n  load:\n    - skills/rust-skills/SKILL.md\n",
    );
    write(&agent.join("Rules/base.md"), "# base\n");
    write(
        &agent.join("Skills/rust-skills/SKILL.md"),
        "---\nname: rust-skills\n---\n# Rust\n",
    );

    let view = load_agent_dir_view(agent).expect("view must load");
    assert!(view.found);
    assert_eq!(view.manifest.unwrap().name, "Legacy");
    assert_eq!(view.rules.len(), 1);
    assert_eq!(view.rules[0].name, "base");
    assert!(
        view.skills
            .iter()
            .any(|s| s.name == "rust-skills" && s.enabled),
        "capitalized Skills/ must resolve on case-sensitive filesystems"
    );

    let context = resolve_agent_context(agent, "demo", Path::new("."), "c2c_case", &[]).unwrap();
    assert!(context.rules.iter().any(|r| r.name == "base"));
    assert!(context.skills.iter().any(|s| s == "rust-skills"));
}

#[test]
fn discovered_agent_skills_are_loaded_into_context() {
    let root = TempDir::new().unwrap();
    let agent = root.path();
    write(
        &agent.join("agent.yaml"),
        "version: \"1\"\nname: \"Fixture\"\n",
    );
    write(&agent.join("rules/base.md"), "# base\n");
    write(
        &agent.join("skills/rust-skills/SKILL.md"),
        "---\nname: rust-skills\ndescription: Rust guidelines\nversion: 1.5.1\n---\n# Rust\n",
    );
    write(
        &agent.join("skills/Design Pattern Review/SKILL.md"),
        "---\nname: design-pattern-review\ndescription: Pattern review\n---\n# Patterns\n",
    );

    let view = load_agent_dir_view(agent).unwrap();
    assert!(
        view.skills
            .iter()
            .any(|s| s.name == "rust-skills" && s.enabled),
        "discovered: {:?}",
        view.skills
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>()
    );
    assert!(view
        .skills
        .iter()
        .any(|s| s.name == "design-pattern-review" && s.enabled));

    let context = resolve_agent_context(agent, "demo", Path::new("."), "c2c_fixture", &[]).unwrap();
    assert!(context.skills.iter().any(|s| s == "rust-skills"));
    assert!(context.skills.iter().any(|s| s == "design-pattern-review"));
    assert_eq!(context.rules.len(), 1);
}

#[test]
fn installed_agent_root_skills_are_loaded_into_context() {
    let root = agentbridge::config::agent_root().expect("agent root must resolve");
    let view = load_agent_dir_view(&root).expect("installed agent root must load");

    let discovered: Vec<(&str, bool)> = view
        .skills
        .iter()
        .map(|s| (s.name.as_str(), s.enabled))
        .collect();
    assert!(
        view.skills
            .iter()
            .any(|s| s.name == "rust-skills" && s.enabled),
        "rust-skills must be discovered and enabled; discovered: {discovered:?}"
    );
    assert!(
        view.skills
            .iter()
            .any(|s| s.name == "design-pattern-review" && s.enabled),
        "design-pattern-review must be discovered and enabled; discovered: {discovered:?}"
    );

    let context = resolve_agent_context(&root, "agentbridge", Path::new("."), "c2c_installed", &[])
        .expect("context must resolve");
    assert!(context.skills.iter().any(|s| s == "rust-skills"));
    assert!(context.skills.iter().any(|s| s == "design-pattern-review"));
}

#[test]
fn task_state_persists_without_workspace_current_c2c() {
    let workspace = TempDir::new().unwrap();
    fs::write(workspace.path().join("README.md"), "x\n").unwrap();

    let storage = common::test_storage();
    let cfg = Arc::new(Config::new(workspace.path().to_path_buf()));
    let hub = Arc::new(common::hub_with(
        cfg,
        storage.clone(),
        vec![common::project_entry(
            "alpha",
            workspace.path().to_path_buf(),
        )],
    ));
    let service = TaskService::new(hub, storage.clone());

    let state = service
        .plan_task(
            "alpha",
            "goal".into(),
            vec!["cargo test".into()],
            "opencode",
        )
        .unwrap();

    let task_id = state.task_id.clone().unwrap();
    let (owner, stored) = storage
        .find_task_by_id(&task_id)
        .unwrap()
        .expect("task must be persisted in SQLite");
    assert_eq!(owner, "alpha");
    assert_eq!(stored.goal.as_deref(), Some("goal"));

    assert!(
        !workspace.path().join(".agentbridge/current.c2c").exists(),
        "Task/C2C state must not be persisted into the workspace"
    );
}
