use std::path::PathBuf;
use anyhow::{bail, Result};
use clap::Subcommand;

use agentbridge::projects::{self, ProjectEntry};

#[derive(Subcommand)]
pub enum ProjectCmd {
    /// List projects from a workspace file
    List {
        #[arg(long, default_value = "agentbridge.config.json")]
        workspaces: PathBuf,
    },
    /// Add a project to a workspace file
    Add {
        name: String,
        path: PathBuf,
        #[arg(long, default_value = "agentbridge.config.json")]
        workspaces: PathBuf,
        #[arg(long, default_value = "")]
        description: String,
        #[arg(long)]
        readonly: bool,
        #[arg(long)]
        default: bool,
    },
    /// Remove a project from a workspace file
    Remove {
        name: String,
        #[arg(long, default_value = "agentbridge.config.json")]
        workspaces: PathBuf,
    },
}

pub fn run(command: ProjectCmd) -> Result<()> {
    match command {
        ProjectCmd::List { workspaces } => {
            let (projects, default) = projects::load_workspaces_file(&workspaces)?;
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
        ProjectCmd::Add { name, path, workspaces, description, readonly, default } => {
            let path = std::path::absolute(path)?;
            if !path.is_dir() { bail!("project path is not a directory: {}", path.display()); }
            let (mut list, current_default) = if workspaces.is_file() {
                projects::load_workspaces_file(&workspaces)?
            } else { (Vec::new(), None) };
            let name = projects::validate_project_name(&name)?;
            if list.iter().any(|p| p.name == name) { bail!("project `{name}` already exists"); }
            list.push(ProjectEntry {
                id: String::new(), name: name.clone(), path, description, readonly,
                executor: "opencode".into(),
            });
            let default = if default || current_default.is_none() { Some(name.clone()) } else { current_default };
            projects::save_workspaces_file(&workspaces, &list, default)?;
            println!("added project `{name}` to {}", workspaces.display());
            Ok(())
        }
        ProjectCmd::Remove { name, workspaces } => {
            let (mut list, default) = projects::load_workspaces_file(&workspaces)?;
            let before = list.len();
            list.retain(|project| !project.name.eq_ignore_ascii_case(&name));
            if list.len() == before { bail!("project `{name}` was not found"); }
            if list.is_empty() { bail!("cannot remove the last project"); }
            let default = if default.as_deref().is_some_and(|d| d.eq_ignore_ascii_case(&name)) {
                Some(list[0].name.clone())
            } else { default };
            projects::save_workspaces_file(&workspaces, &list, default)?;
            println!("removed project `{name}` from {}", workspaces.display());
            Ok(())
        }
    }
}