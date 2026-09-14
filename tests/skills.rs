//! Skill System integration tests:
//! Registry, Lifecycle, Provider, Resolver, Project Policy, C2C, and Security.

use std::fs;
use std::path::Path;
use tempfile::TempDir;

use agentbridge::core::skill::{parse_skill_markdown, SkillService};
use agentbridge::models::InstallSkillRequest;
use agentbridge::protocol::{C2cMessage, C2cPlan};

mod common;

fn create_sample_skill_dir() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("SKILL.md"),
        "# Rust Architecture Skill\n\nGuidelines for clean, decoupled Rust architecture with strict file size budgets.\n\n## Rules\n1. No mod.rs\n2. Thin controllers\n",
    ).unwrap();
    fs::create_dir_all(dir.path().join("patterns")).unwrap();
    fs::write(
        dir.path().join("patterns/facade.md"),
        "Facade pattern guide\n",
    )
    .unwrap();
    fs::write(dir.path().join("report-template.md"), "# Report Template\n").unwrap();
    dir
}

#[test]
fn test_parse_skill_markdown_extracts_title_and_description() {
    let markdown = "# Design Pattern Review\n\nReview code against standard Gang of Four patterns.\n\n## Details\n...";
    let (name, desc, _version) = parse_skill_markdown(markdown, "default");
    assert_eq!(name, "Design Pattern Review");
    assert_eq!(desc, "Review code against standard Gang of Four patterns.");
}

#[test]
fn test_parse_skill_frontmatter_name_version_description() {
    let markdown =
        "---\nname: my-skill\nversion: 2.3.1\ndescription: Does a useful thing\n---\n# Body\n";
    let (name, desc, version) = parse_skill_markdown(markdown, "fallback");
    assert_eq!(name, "my-skill");
    assert_eq!(desc, "Does a useful thing");
    assert_eq!(version, "2.3.1");
}

fn agent_backed_service(agent_root: &Path) -> SkillService {
    let storage = common::test_storage();
    let skills_dir = agent_root.join("skills");
    SkillService::new(storage, skills_dir).with_agent_root(agent_root.to_path_buf())
}

#[test]
fn test_agent_directory_rules_and_skills_are_separated() {
    let agent_root = TempDir::new().unwrap();
    let agent = agent_root.path().to_path_buf();
    fs::create_dir_all(agent.join("rules")).unwrap();
    fs::create_dir_all(agent.join("skills/interaction-decision")).unwrap();
    fs::create_dir_all(agent.join("skills/standard")).unwrap();

    fs::write(
        agent.join("agent.yaml"),
        "version: \"1.0.0\"\nname: \"Demo Agent\"\ndescription: \"demo\"\nglobal_rules:\n  - \"rules/base.md\"\nactive_skills:\n  - \"interaction-decision\"\n",
    )
    .unwrap();
    fs::write(agent.join("rules/base.md"), "# Base\nrule body\n").unwrap();
    fs::write(agent.join("rules/extra.md"), "# Extra\n").unwrap();
    fs::write(
        agent.join("skills/interaction-decision/skill.yaml"),
        "name: \"interaction-decision\"\nversion: \"2.0.0\"\ndescription: \"Pick an action\"\n",
    )
    .unwrap();
    fs::write(
        agent.join("skills/interaction-decision/system.md"),
        "# Role\nDecide the action.\n",
    )
    .unwrap();
    fs::write(
        agent.join("skills/standard/SKILL.md"),
        "---\nname: standard-skill\nversion: 0.9.0\ndescription: A standard skill\n---\n# Standard\n",
    )
    .unwrap();

    let service = agent_backed_service(&agent);
    let view = service.agent_view(None).unwrap();
    assert!(view.found);
    let manifest = view.manifest.clone().unwrap();
    assert_eq!(manifest.name, "Demo Agent");

    // agent.yaml, rules, and skills stay strictly separate.
    let rule_names: Vec<_> = view.rules.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(rule_names, vec!["base", "extra"]);
    assert_eq!(view.skills.len(), 2);
    assert!(view.skills.iter().all(|s| s.source == "agent"));

    // `skill.yaml` + `system.md` layout resolves through the shared service.
    let detail = service
        .get_skill_detail_for(None, "interaction-decision")
        .unwrap();
    assert_eq!(detail.source, "agent");
    assert!(detail.content.contains("Decide the action"));
    assert_eq!(detail.version, "2.0.0");

    // Standard `SKILL.md` frontmatter keeps working.
    let standard = service
        .get_skill_detail_for(None, "standard-skill")
        .unwrap();
    assert_eq!(standard.version, "0.9.0");
    assert!(standard.content.contains("# Standard"));

    // Candidate resolution honors `active_skills`.
    let active = service.resolve_candidates(None, "interaction");
    assert!(active.iter().any(|s| s.name == "interaction-decision"));
    let all = service.resolve_candidates(None, "");
    assert!(all.iter().all(|s| s.name != "standard-skill"));
}

#[test]
fn test_missing_agent_directory_is_not_an_error() {
    let base = TempDir::new().unwrap();
    let missing = base.path().join("not-created");
    let service = agent_backed_service(&missing);
    let view = service.agent_view(None).unwrap();
    assert!(!view.found);
    assert!(view.skills.is_empty());
    assert!(view.rules.is_empty());
}

#[test]
fn test_skill_install_list_show_and_remove() {
    let storage = common::test_storage();
    let temp_skills_dir = TempDir::new().unwrap();
    let service = SkillService::new(storage.clone(), temp_skills_dir.path().to_path_buf());
    let sample = create_sample_skill_dir();

    // 1. 安装 Skill
    let installed = service
        .install_skill(InstallSkillRequest {
            source: sample.path().display().to_string(),
            name_override: None,
        })
        .expect("install should succeed");
    assert_eq!(installed.name, "Rust Architecture Skill");
    assert!(installed.enabled);

    // 2. 重复安装同名 Skill 必须被拦截
    let dup = service.install_skill(InstallSkillRequest {
        source: sample.path().display().to_string(),
        name_override: Some("Rust Architecture Skill".into()),
    });
    assert!(dup.is_err());

    // 3. 列表查询
    let list = service.list_skills().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "Rust Architecture Skill");

    // 4. 详情查询（读取 SKILL.md 正文与资源列表）
    let detail = service.get_skill_detail("Rust Architecture Skill").unwrap();
    assert_eq!(detail.name, "Rust Architecture Skill");
    assert!(detail.content.contains("# Rust Architecture Skill"));
    assert!(detail.resources.contains(&"patterns".to_string()));
    assert!(detail.resources.contains(&"report-template.md".to_string()));

    // 5. 卸载删除
    service.remove_skill("Rust Architecture Skill").unwrap();
    assert!(service.list_skills().unwrap().is_empty());
    assert!(service.get_skill_detail("Rust Architecture Skill").is_err());
}

#[test]
fn test_nested_repository_skill_auto_discovery() {
    let storage = common::test_storage();
    let temp_skills_dir = TempDir::new().unwrap();
    let service = SkillService::new(storage.clone(), temp_skills_dir.path().to_path_buf());

    // 模拟开源仓库两层嵌套结构 (repo/inner_folder/SKILL.md)
    let outer_repo = TempDir::new().unwrap();
    let inner_skill = outer_repo.path().join("design-pattern-review");
    fs::create_dir_all(&inner_skill).unwrap();
    fs::write(
        inner_skill.join("SKILL.md"),
        "# Design Pattern Review\n\nAutomated design pattern checker.\n",
    )
    .unwrap();

    // 指向仓库根目录安装，能够自动探测到内层的真正的 Skill 目录
    let installed = service
        .install_skill(InstallSkillRequest {
            source: outer_repo.path().display().to_string(),
            name_override: None,
        })
        .expect("should auto-discover nested skill directory");
    assert_eq!(installed.name, "Design Pattern Review");
}

#[test]
fn test_skill_enable_disable_lifecycle() {
    let storage = common::test_storage();
    let temp_skills_dir = TempDir::new().unwrap();
    let service = SkillService::new(storage.clone(), temp_skills_dir.path().to_path_buf());
    let sample = create_sample_skill_dir();

    service
        .install_skill(InstallSkillRequest {
            source: sample.path().display().to_string(),
            name_override: Some("Rust-Arch".into()),
        })
        .unwrap();

    assert!(service.get_skill_detail("Rust-Arch").unwrap().enabled);

    // 禁用
    service.set_enabled("Rust-Arch", false).unwrap();
    assert!(!service.get_skill_detail("Rust-Arch").unwrap().enabled);

    // 重新启用
    service.set_enabled("Rust-Arch", true).unwrap();
    assert!(service.get_skill_detail("Rust-Arch").unwrap().enabled);
}

#[test]
fn test_skill_resolver_candidate_matching() {
    let sample_a = create_sample_skill_dir();
    let sample_b = TempDir::new().unwrap();
    fs::write(
        sample_b.path().join("SKILL.md"),
        "# SEO Audit Skill\n\nAudit website performance, meta tags, and robots.txt.\n",
    )
    .unwrap();

    let storage = common::test_storage();
    let temp_skills_dir = TempDir::new().unwrap();
    let service = SkillService::new(storage.clone(), temp_skills_dir.path().to_path_buf());

    service
        .install_skill(InstallSkillRequest {
            source: sample_a.path().display().to_string(),
            name_override: Some("rust-arch".into()),
        })
        .unwrap();

    service
        .install_skill(InstallSkillRequest {
            source: sample_b.path().display().to_string(),
            name_override: Some("seo-audit".into()),
        })
        .unwrap();

    // 1. 匹配关键词 "architecture"
    let candidates =
        service.resolve_candidates(None, "Refactor task runtime with clean architecture");
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].name, "rust-arch");

    // 2. 匹配关键词 "seo"
    let candidates = service.resolve_candidates(None, "Inspect sitemap and meta tags for SEO");
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].name, "seo-audit");

    // 3. 无关任务返回空
    let candidates = service.resolve_candidates(None, "quantum computing algorithm");
    assert!(candidates.is_empty());
}

#[test]
fn test_project_skill_policy_override() {
    let sample = create_sample_skill_dir();
    let storage = common::test_storage();
    let temp_skills_dir = TempDir::new().unwrap();
    let service = SkillService::new(storage.clone(), temp_skills_dir.path().to_path_buf());

    let installed = service
        .install_skill(InstallSkillRequest {
            source: sample.path().display().to_string(),
            name_override: Some("arch-skill".into()),
        })
        .unwrap();

    // 全局默认可用
    let candidates = service.resolve_candidates(Some("Project-A"), "architecture");
    assert_eq!(candidates.len(), 1);

    // Project-A 显式配置禁用该 Skill
    storage
        .set_project_skill_policy("Project-A", &installed.id, false)
        .unwrap();
    let candidates_a = service.resolve_candidates(Some("Project-A"), "architecture");
    assert!(candidates_a.is_empty(), "Project-A 的策略覆盖应该生效");

    // Project-B 未配置覆盖，仍然正常使用该 Skill
    let candidates_b = service.resolve_candidates(Some("Project-B"), "architecture");
    assert_eq!(candidates_b.len(), 1);
}

#[test]
fn test_c2c_plan_skills_serialization_and_roundtrip() {
    let plan = C2cPlan::with_skills(
        "c2c_12345".into(),
        1,
        "Refactor architecture".into(),
        vec!["rust-architecture".into(), "design-pattern".into()],
        vec!["Step 1".into()],
        vec!["cargo test".into()],
        "Tests pass".into(),
    )
    .unwrap();

    let msg = plan.to_message();
    let rendered = msg.render();

    assert!(rendered.contains("SKILLS:\nrust-architecture\ndesign-pattern"));
    assert!(
        !rendered.contains("# Rust Architecture Skill"),
        "C2C 协议中严禁携带完整 Skill 正文"
    );

    let parsed = C2cMessage::parse(&rendered).unwrap();
    assert_eq!(
        parsed.skills.as_deref(),
        Some("rust-architecture\ndesign-pattern")
    );
}
