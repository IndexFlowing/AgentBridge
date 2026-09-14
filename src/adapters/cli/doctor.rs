// src/cli/doctor.rs
use crate::config;
use crate::doctor;
use anyhow::Result;

pub fn run() -> Result<()> {
    let (cfg, config_path) = config::load_or_create_user_config()?;

    let checks = doctor::run(Some(&cfg), Some(&config_path))?;

    if doctor::print_report(&checks) {
        Ok(())
    } else {
        anyhow::bail!("doctor found problems")
    }
}
