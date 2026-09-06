pub mod executor;
pub mod project;
pub mod proxy;
pub mod system;
pub mod task;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use agentbridge::config::{self, Config};
use agentbridge::projects::{self, ProjectHub};

pub fn load() -> Result<(Config, PathBuf), String> {
    config::find_config(None).map_err(|e| e.to_string())
}

pub fn hub(cfg: &Config, config_path: &Path) -> Result<ProjectHub, String> {
    let (entries, default) = projects::discover_from_config(config_path, &cfg.workspace)
        .map_err(|e| e.to_string())?;
    ProjectHub::open(entries, default, Arc::new(cfg.clone())).map_err(|e| e.to_string())
}