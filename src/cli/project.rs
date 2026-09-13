use std::path::{Path, PathBuf};
use anyhow::{bail, Result};
use clap::Subcommand;

use agentbridge::config;
use agentbridge::projects::{self, ProjectUpsert, PROJECTS_JSON_LEGACY};

#[derive(Subcommand)]
pub enum ProjectCmd {
    /// List projects from a workspace file
    List {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        workspaces: Option<PathBuf>,
    },
    /// Add a project to a workspace file
    Add {
        name: String,
        path: PathBuf,
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        workspaces: Option<PathBuf>,
        #[arg(long, default_value = "")]
        description: String,
        #[arg(long)]
        readonly: bool,
        #[arg(long)]
        default: bool,
        #[arg(long)]
        executor: Option<String>,
    },
    /// Remove a project from a workspace file
    Remove {
        name: String,
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        workspaces: Option<PathBuf>,
    },
}

pub fn run(command: ProjectCmd) -> Result<()> {
    match command {
        ProjectCmd::List { config, workspaces } => {
            let file = resolve_projects_file(config.as_deref(), workspaces.as_deref())?;
            let (projects, default) = projects::load_workspaces_file(&file)?;
            for project in projects {
                println!(
                    "{}\t{}\t{}{}",
                    project.name,
                    project.path.display(),
                    if project.readonly { "readonly" } else { "writable" },
                    if default.as_deref() == Some(project.name.as_str()) { "\tdefault" } else { "" }
                );
            }
            Ok(())
        }
        ProjectCmd::Add {
            name,
            path,
            config,
            workspaces,
            description,
            readonly,
            default,
            executor,
        } => {
            let file = resolve_projects_file(config.as_deref(), workspaces.as_deref())?;
            let (entries, _) = projects::upsert_project(
                &file,
                ProjectUpsert {
                    name,
                    path,
                    description: Some(description),
                    readonly: Some(readonly),
                    make_default: default,
                    executor,
                    ..Default::default()
                },
            )?;
            let added = entries.last().map(|e| e.name.as_str()).unwrap_or_default();
            println!("added project `{added}` to {}", file.display());
            Ok(())
        }
        ProjectCmd::Remove { name, config, workspaces } => {
            let file = resolve_projects_file(config.as_deref(), workspaces.as_deref())?;
            projects::remove_project(&file, &name)?;
            println!("removed project `{name}` from {}", file.display());
            Ok(())
        }
    }
}

fn resolve_projects_file(config: Option<&Path>, workspaces: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = workspaces {
        return Ok(path.to_path_buf());
    }
    if let Ok((_, cfg_path)) = config::find_config(config) {
        return Ok(projects::projects_file_for_config(&cfg_path));
    }
    if config.is_some() {
        bail!("no AgentBridge config found for --config");
    }
    Ok(PathBuf::from(PROJECTS_JSON_LEGACY))
}
