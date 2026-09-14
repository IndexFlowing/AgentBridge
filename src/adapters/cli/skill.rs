// src/adapters/cli/skill.rs
//! CLI command implementation for `agentbridge skill ...`

use anyhow::Result;
use clap::Subcommand;
use std::sync::Arc;

use crate::core::skill::SkillService;
use crate::models::InstallSkillRequest;
use crate::storage::Storage;

#[derive(Subcommand)]
pub enum SkillCmd {
    /// List all installed skills
    List,
    /// Show details and full SKILL.md content
    Show {
        name: String,
    },
    /// Install a skill from a local directory
    Install {
        source: String,
        #[arg(long)]
        name: Option<String>,
    },
    /// Enable a skill globally
    Enable {
        name: String,
    },
    /// Disable a skill globally
    Disable {
        name: String,
    },
    /// Remove an installed skill
    Remove {
        name: String,
    },
}

pub fn run(cmd: SkillCmd) -> Result<()> {
    let storage = Storage::init()?;
    let service = SkillService::new(Arc::new(storage));

    match cmd {
        SkillCmd::List => {
            let skills = service.list_skills()?;
            if skills.is_empty() {
                println!("No skills installed. Run `agentbridge skill install <path>` to install one.");
                return Ok(());
            }
            println!("{:<24} {:<8} {:<10} {}", "NAME", "ENABLED", "VERSION", "DESCRIPTION");
            println!("{}", "-".repeat(70));
            for s in skills {
                let status = if s.enabled { "yes" } else { "no" };
                println!("{:<24} {:<8} {:<10} {}", s.name, status, s.version, s.description);
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
            println!("skill       installed '{}' (v{})", installed.name, installed.version);
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
    }
    Ok(())
}