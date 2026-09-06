use std::path::PathBuf;
use anyhow::{bail, Result};

use agentbridge::config;
use agentbridge::doctor;

pub fn run(config_path: Option<PathBuf>) -> Result<()> {
    let loaded = config::find_config(config_path.as_deref()).ok();
    let (cfg, path) = match &loaded {
        Some((c, p)) => (Some(c), Some(p.as_path())),
        None => (None, None),
    };
    let checks = doctor::run(cfg, path)?;
    if doctor::print_report(&checks) {
        Ok(())
    } else {
        bail!("doctor found problems")
    }
}