// src/adapters/cli/skill.rs
//! CLI command implementation for `agentbridge skill ...`

use anyhow::Result;
use clap::Subcommand;
use std::sync::Arc;

use crate::config;
use crate::core::AppCore;
use crate::models::InstallSkillRequest;

#[derive(Subcommand)]
pub enum SkillCmd {
    /// List all installed skills
    List,
    /// Show details and full SKILL.md content
    Show { name: String },
    /// Install a skill from a local directory
    Install {
        source: String,
        #[arg(long)]
        name: Option<String>,
    },
    /// Enable a skill globally
    Enable { name: String },
    /// Disable a skill globally
    Disable { name: String },
    /// Remove an installed skill
    Remove { name: String },
    /// Show the project `.agent` model (agent.yaml, rules, skills)
    Agent {
        #[arg(long)]
        project: Option<String>,
    },
    /// Print a `.agent/rules` document
    Rule {
        name: String,
        #[arg(long)]
        project: Option<String>,
    },
}

pub fn run(cmd: SkillCmd) -> Result<()> {
    let (cfg, _config_path) = config::load_or_create_user_config()?;
    let core = AppCore::bootstrap(Arc::new(cfg))?;
    let service = core.skills.clone();

    match cmd {
        SkillCmd::List => {
            let skills = service.list_skills()?;
            if skills.is_empty() {
                println!(
                    "No skills installed. Run `agentbridge skill install <path>` to install one."
                );
                return Ok(());
            }
            println!(
                "{:<24} {:<8} {:<10} {}",
                "NAME", "ENABLED", "VERSION", "DESCRIPTION"
            );
            println!("{}", "-".repeat(70));
            for s in skills {
                let status = if s.enabled { "yes" } else { "no" };
                println!(
                    "{:<24} {:<8} {:<10} {}",
                    s.name, status, s.version, s.description
                );
            }
        }
        SkillCmd::Show { name } => {
            let detail = service.get_skill_detail(&name)?;
            println!("name        {}", detail.name);
            println!("version     {}", detail.version);
            println!("enabled     {}", detail.enabled);
            println!("path        {}", detail.path);
            println!("source      {}", detail.source);
            if !detail.resources.is_empty() {
                println!("resources   {}", detail.resources.join(", "));
            }
            println!("\n--- SKILL.md ---\n{}", detail.content);
        }
        SkillCmd::Install { source, name } => {
            let req = InstallSkillRequest {
                source,
                name_override: name,
            };
            let installed = service.install_skill(req)?;
            println!(
                "skill       installed '{}' (v{})",
                installed.name, installed.version
            );
            println!("path        {}", installed.path);
        }
        SkillCmd::Enable { name } => {
            service.set_enabled(&name, true)?;
            println!("skill       enabled '{}'", name);
        }
        SkillCmd::Disable { name } => {
            service.set_enabled(&name, false)?;
            println!("skill       disabled '{}'", name);
        }
        SkillCmd::Remove { name } => {
            service.remove_skill(&name)?;
            println!("skill       removed '{}'", name);
        }
        SkillCmd::Agent { project } => {
            let view = service.agent_view(project.as_deref())?;
            if !view.found {
                println!("No .agent directory found for the selected project.");
                return Ok(());
            }
            let manifest = view.manifest.unwrap_or_default();
            println!("agent       {}", view.agent_dir);
            if !manifest.name.is_empty() {
                println!("name        {}", manifest.name);
            }
            if !manifest.version.is_empty() {
                println!("version     {}", manifest.version);
            }
            if !manifest.description.is_empty() {
                println!("description {}", manifest.description);
            }
            println!("rules       {}", view.rules.len());
            for rule in &view.rules {
                println!("  - {} ({})", rule.name, rule.path);
            }
            println!("skills      {}", view.skills.len());
            for skill in &view.skills {
                let status = if skill.enabled { "active" } else { "inactive" };
                println!("  - {} v{} [{}]", skill.name, skill.version, status);
            }
        }
        SkillCmd::Rule { name, project } => {
            let view = service.agent_view(project.as_deref())?;
            let rule = view
                .rules
                .iter()
                .find(|r| r.name == name || r.path == name)
                .ok_or_else(|| anyhow::anyhow!("rule '{name}' not found in .agent/rules"))?;
            println!("--- {} ---\n{}", rule.path, rule.content);
        }
    }
    Ok(())
}
