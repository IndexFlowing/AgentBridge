pub mod doctor;
pub mod executor;
pub mod init;
pub mod project;
pub mod proxy;
pub mod serve;
pub mod status;
pub mod task;

use std::path::PathBuf;
use anyhow::Result;
use clap::{Parser, Subcommand};

use agentbridge::config;

#[derive(Parser)]
#[command(
    name = "agentbridge",
    version,
    about = "Connect AI brains to local coding agents through MCP",
    long_about = "AgentBridge exposes a local workspace to a remote AI (the Brain) over MCP. \
The Brain inspects with read-only tools and starts OpenCode (the Executor) with a C2C PLAN. \
The Executor is the only component that writes files or runs commands."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Create a local AgentBridge config for a workspace
    Init {
        workspace: Option<PathBuf>,
        #[arg(long, default_value_t = config::DEFAULT_PORT)]
        port: u16,
        #[arg(long)]
        local: bool,
    },
    /// Start the MCP server
    Serve {
        #[arg(value_name = "DIR")]
        dir: Option<PathBuf>,
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        workspaces: Option<PathBuf>,
        #[arg(long)]
        workspace: Option<PathBuf>,
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
    /// Show workspace, git, and latest Executor state
    Status {
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Check workspace, git, port, and optional cloudflared
    Doctor {
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Record Brain/Executor C2C task state
    Task {
        #[command(subcommand)]
        command: task::TaskCmd,
    },
    /// Manage mounted projects in agentbridge.config.json
    Project {
        #[command(subcommand)]
        command: project::ProjectCmd,
    },
    /// Discover and manage local executor installations
    Executor {
        #[command(subcommand)]
        command: executor::ExecutorCmd,
    },
    /// Show, configure, or test the executor proxy
    Proxy {
        #[command(subcommand)]
        command: proxy::ProxyCmd,
    },
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Init { workspace, port, local } => init::run(workspace, port, local),
        Commands::Serve {
            dir, config, workspaces, workspace, host, port,
            allow_any_host, auth_token, no_auth, dev,
            client_id, client_secret, admin_password,
        } => {
            init_tracing();
            serve::run(serve::ServeArgs {
                dir, config, workspaces, workspace, host, port,
                allow_any_host, auth_token, no_auth: no_auth || dev,
                client_id, client_secret, admin_password,
            })
        }
        Commands::Status { config } => status::run(config),
        Commands::Doctor { config } => doctor::run(config),
        Commands::Task { command } => task::run(command),
        Commands::Project { command } => project::run(command),
        Commands::Executor { command } => executor::run(command),
        Commands::Proxy { command } => proxy::run(command),
    }
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "info,rmcp=warn".into());
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}