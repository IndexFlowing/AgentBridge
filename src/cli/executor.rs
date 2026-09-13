use std::path::{Path, PathBuf};
use anyhow::Result;
use clap::Subcommand;

use agentbridge::config::{self, ExecutorUpsert};
use agentbridge::executor;

#[derive(Subcommand)]
pub enum ExecutorCmd {
    /// List saved executors and PATH-discovered candidates
    List { #[arg(long)] config: Option<PathBuf> },
    /// Add an executor to the local registry
    Add {
        name: String,
        #[arg(long, value_name = "TYPE")]
        kind: String,
        #[arg(long)]
        command: String,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Remove a saved executor by stable ID
    Remove { id: String, #[arg(long)] config: Option<PathBuf> },
    /// Probe saved executors or one executor by ID
    Test { #[arg(long)] id: Option<String>, #[arg(long)] config: Option<PathBuf> },
}

pub fn run(command: ExecutorCmd) -> Result<()> {
    match command {
        ExecutorCmd::List { config } => {
            let config_path = resolve_config_path(config.as_deref());
            for view in executor::list_views(&config_path)? {
                println!(
                    "{}\t{}\t{}\t{}",
                    view.definition.id,
                    view.definition.display_name,
                    view.definition.kind,
                    if view.detected {
                        format!("discovered:{}", view.availability.status.as_str())
                    } else {
                        format!("saved:{}", view.availability.status.as_str())
                    }
                );
            }
            Ok(())
        }
        ExecutorCmd::Add { name, kind, command, config } => {
            let config_path = resolve_config_path(config.as_deref());
            let registry = config::upsert_executor(
                &config_path,
                ExecutorUpsert {
                    name,
                    kind,
                    command,
                    ..Default::default()
                },
            )?;
            let id = registry.executors.last().map(|e| e.id.as_str()).unwrap_or_default();
            println!(
                "added executor {id} to {}",
                config::executor_registry_path(&config_path).display()
            );
            Ok(())
        }
        ExecutorCmd::Remove { id, config } => {
            let config_path = resolve_config_path(config.as_deref());
            config::remove_executor(&config_path, &id)?;
            println!("removed executor {id}");
            Ok(())
        }
        ExecutorCmd::Test { id, config } => {
            let config_path = resolve_config_path(config.as_deref());
            let views = executor::list_views(&config_path)?;
            let mut found = false;
            for view in views {
                if id.as_deref().is_some_and(|wanted| wanted != view.definition.id) {
                    continue;
                }
                found = true;
                println!(
                    "{}\t{}\t{}",
                    view.definition.id,
                    view.definition.display_name,
                    view.availability.status.as_str()
                );
            }
            if !found {
                anyhow::bail!("no executor matched the requested ID");
            }
            Ok(())
        }
    }
}

fn resolve_config_path(explicit: Option<&Path>) -> PathBuf {
    config::sidecar_config_path(explicit)
}
