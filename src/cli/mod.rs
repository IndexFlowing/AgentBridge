// src/cli/mod.rs
pub mod doctor;
pub mod serve;
pub mod service;
pub mod task;
pub mod workspace;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "agentbridge",
    version,
    about = "Connect AI brains to local coding agents through MCP"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start the MCP server and Web Control Plane
    Serve {
        #[arg(long)]
        host: Option<String>,
        #[arg(long)]
        port: Option<u16>,
        #[arg(long)]
        allow_any_host: bool,
        #[arg(long)]
        auth_token: Option<String>,
        #[arg(long)]
        no_auth: bool,
        #[arg(long)]
        dev: bool,
        #[arg(long)]
        client_id: Option<String>,
        #[arg(long)]
        client_secret: Option<String>,
        #[arg(long)]
        admin_password: Option<String>,
    },
    /// Start the AgentBridge service in the background
    Start {
        #[arg(long)]
        host: Option<String>,
        #[arg(long)]
        port: Option<u16>,
    },
    /// Stop the background AgentBridge service
    Stop {
        /// Kill the recorded PID even if the health check does not confirm it
        #[arg(long)]
        force: bool,
    },
    /// Restart the background AgentBridge service (stop + start)
    Restart {
        #[arg(long)]
        host: Option<String>,
        #[arg(long)]
        port: Option<u16>,
        /// Force the stop step when stopping an unresponsive service
        #[arg(long)]
        force: bool,
    },
    /// Show background service lifecycle status (running/stopped, PID, address)
    Status,
    /// Show current default workspace status
    Workspace,
    /// Check runtime dependencies
    Doctor,
    /// Record Brain/Executor C2C task state (Will be DB-driven soon)
    Task {
        #[command(subcommand)]
        command: task::TaskCmd,
    },
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Serve {
            host,
            port,
            allow_any_host,
            auth_token,
            no_auth,
            dev,
            client_id,
            client_secret,
            admin_password,
        } => serve::run(serve::ServeArgs {
            host,
            port,
            allow_any_host,
            auth_token,
            no_auth: no_auth || dev,
            client_id,
            client_secret,
            admin_password,
        }),
        Commands::Start { host, port } => service::start(host, port),
        Commands::Stop { force } => service::stop(force),
        Commands::Restart { host, port, force } => service::restart(host, port, force),
        Commands::Status => service::status(),
        Commands::Workspace => workspace::run(),
        Commands::Doctor => doctor::run(),
        Commands::Task { command } => task::run(command),
    }
}

pub(crate) fn init_tracing(level: &str) {
    let directive = if level.trim().is_empty() {
        "info,rmcp=warn".to_string()
    } else {
        format!("{},rmcp=warn", level.trim())
    };
    let filter =
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| directive.into());
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}
