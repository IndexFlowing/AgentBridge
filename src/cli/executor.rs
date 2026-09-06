use std::path::PathBuf;
use anyhow::{bail, Result};
use clap::Subcommand;

use agentbridge::config::{self, ExecutorDefinition};
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
            let config_path = config.unwrap_or_else(config::project_config_path);
            let registry = config::load_executor_registry(&config_path)?;
            for (definition, discovered) in executor::executor_definitions_with_discovery(&registry.executors) {
                let availability = executor::scan_executor(&definition);
                println!("{}\t{}\t{}\t{}", definition.id, definition.display_name, definition.kind,
                    if discovered { format!("discovered:{}", availability.status.as_str()) } 
                    else { format!("saved:{}", availability.status.as_str()) });
            }
            Ok(())
        }
        ExecutorCmd::Add { name, kind, command, config } => {
            let config_path = config.unwrap_or_else(config::project_config_path);
            let mut registry = config::load_executor_registry(&config_path)?;
            let definition = ExecutorDefinition::new(name, kind, command);
            let id = definition.id.clone();
            registry.executors.push(definition);
            config::save_executor_registry(&config_path, &registry)?;
            println!("added executor {id} to {}", config::executor_registry_path(&config_path).display());
            Ok(())
        }
        ExecutorCmd::Remove { id, config } => {
            let config_path = config.unwrap_or_else(config::project_config_path);
            let mut registry = config::load_executor_registry(&config_path)?;
            let before = registry.executors.len();
            registry.executors.retain(|executor| executor.id != id);
            if before == registry.executors.len() { bail!("executor `{id}` was not found"); }
            config::save_executor_registry(&config_path, &registry)?;
            println!("removed executor {id}");
            Ok(())
        }
        ExecutorCmd::Test { id, config } => {
            let config_path = config.unwrap_or_else(config::project_config_path);
            let registry = config::load_executor_registry(&config_path)?;
            let entries = executor::executor_definitions_with_discovery(&registry.executors);
            let mut found = false;
            for (definition, _) in entries {
                if id.as_deref().is_some_and(|wanted| wanted != definition.id) { continue; }
                found = true;
                let result = executor::scan_executor(&definition);
                println!("{}\t{}\t{}", definition.id, definition.display_name, result.status.as_str());
            }
            if !found { bail!("no executor matched the requested ID"); }
            Ok(())
        }
    }
}