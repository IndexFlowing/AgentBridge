// src/cli/doctor.rs
use anyhow::Result;

// 修复点：使用 agentbridge:: 替代 crate::
use agentbridge::config;
use agentbridge::doctor;

pub fn run() -> Result<()> {
    let (cfg, config_path) = config::load_or_create_user_config()?;

    let checks = doctor::run(Some(&cfg), Some(&config_path))?;

    if doctor::print_report(&checks) {
        Ok(())
    } else {
        anyhow::bail!("doctor found problems")
    }
}
