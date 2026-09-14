// src/cli/doctor.rs
use anyhow::Result;
use crate::config;
use crate::doctor;

pub fn run() -> Result<()> {
    let (cfg, config_path) = config::load_or_create_user_config()?;

    let checks = doctor::run(Some(&cfg), Some(&config_path))?;

    if doctor::print_report(&checks) {
        Ok(())
    } else {
        anyhow::bail!("doctor found problems")
    }
}
