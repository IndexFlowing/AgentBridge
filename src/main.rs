use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{bail, Result};
use chrono::Utc;
use clap::{Parser, Subcommand, ValueEnum};

use agentbridge::config::{self, Config};
use agentbridge::protocol::C2cState;
use agentbridge::state::{new_task_id, BridgeState, TaskStatus, TestResult};
use agentbridge::task::{PlanInput, TaskRuntime};
use agentbridge::{doctor, git, server, workspace::Workspace};

#[derive(Parser)]
#[command(
    name = "agentbridge",
    version,
    about = "Connect AI brains to local coding agents through MCP",
    long_about = "AgentBridge exposes a local workspace to a remote AI (the Brain) over MCP. \
The Brain inspects with read-only tools and starts OpenCode (the Executor) with a C2C PLAN. \
The Executor is the only component that writes files or runs commands."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a local AgentBridge config for a workspace
    Init {
        /// Path to the coding workspace (defaults to the current directory)
        workspace: Option<PathBuf>,
        /// MCP listen port
        #[arg(long, default_value_t = config::DEFAULT_PORT)]
        port: u16,
        /// Write only the project-local .agentbridge.toml
        #[arg(long)]
        local: bool,
    },
    /// Start the MCP server (read-only inspect tools + task_start / task_status / task_cancel)
    Serve {
        /// Config file (defaults to ./.agentbridge.toml or ~/.agentbridge/config.toml)
        #[arg(long)]
        config: Option<PathBuf>,
        /// Override workspace path
        #[arg(long)]
        workspace: Option<PathBuf>,
        /// Override listen host (default 127.0.0.1)
        #[arg(long)]
        host: Option<String>,
        /// Override listen port
        #[arg(long)]
        port: Option<u16>,
        /// Disable Host-header allowlist (needed for Cloudflare Tunnel)
        #[arg(long)]
        allow_any_host: bool,
        /// Require Authorization: Bearer <token> on /mcp
        #[arg(long)]
        auth_token: Option<String>,
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
        command: TaskCmd,
    },
}

#[derive(Subcommand)]
enum TaskCmd {
    /// Create a new task and write a PLAN. Use --execute to start OpenCode and wait.
    Start {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        goal: Option<String>,
        #[arg(long)]
        tests: Option<String>,
        /// Start the configured OpenCode executor and wait for it to finish
        #[arg(long)]
        execute: bool,
    },
    /// Record that the Executor finished an iteration
    Executed {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        task_id: Option<String>,
        #[arg(long)]
        iteration: Option<u32>,
        #[arg(long, value_enum)]
        status: ExecStatus,
        #[arg(long)]
        tests: Option<String>,
        #[arg(long)]
        exit_code: Option<i32>,
        /// Comma-separated workspace-relative paths
        #[arg(long)]
        changed_files: Option<String>,
        #[arg(long)]
        summary: Option<String>,
    },
    /// Print the current task as JSON and as a C2C message
    Status {
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Cancel the running OpenCode executor
    Cancel {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        task_id: Option<String>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum ExecStatus {
    Success,
    Failure,
    Blocked,
}

impl ExecStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failed",
            Self::Blocked => "blocked",
        }
    }
}

fn main() -> ExitCode {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        for cause in err.chain().skip(1) {
            eprintln!("  caused by: {cause}");
        }
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Init {
            workspace,
            port,
            local,
        } => cmd_init(workspace, port, local),
        Commands::Serve {
            config,
            workspace,
            host,
            port,
            allow_any_host,
            auth_token,
        } => cmd_serve(config, workspace, host, port, allow_any_host, auth_token),
        Commands::Status { config } => cmd_status(config),
        Commands::Doctor { config } => cmd_doctor(config),
        Commands::Task { command } => cmd_task(command),
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

fn cmd_init(workspace: Option<PathBuf>, port: u16, local_only: bool) -> Result<()> {
    let workspace = match workspace {
        Some(p) => p,
        None => std::env::current_dir()?,
    };
    let workspace = std::path::absolute(&workspace)?;
    if !workspace.is_dir() {
        bail!("workspace does not exist: {}", workspace.display());
    }

    let mut cfg = Config::new(workspace.clone());
    cfg.port = port;

    let project_cfg = workspace.join(".agentbridge.toml");
    cfg.save_to_path(&project_cfg)?;
    println!("wrote {}", project_cfg.display());

    if !local_only {
        let user_cfg = config::user_config_path()?;
        cfg.save_to_path(&user_cfg)?;
        println!("wrote {}", user_cfg.display());
    }

    std::fs::create_dir_all(config::state_dir(&workspace))?;
    println!();
    println!("Workspace: {}", workspace.display());
    println!("Start the Brain endpoint with:");
    println!("  agentbridge serve");
    println!();
    println!("Default MCP URL: {}", cfg.mcp_url());
    Ok(())
}

fn load_cfg(explicit: Option<PathBuf>) -> Result<(Config, PathBuf)> {
    config::find_config(explicit.as_deref())
}

fn cmd_serve(
    config_path: Option<PathBuf>,
    workspace: Option<PathBuf>,
    host: Option<String>,
    port: Option<u16>,
    allow_any_host: bool,
    auth_token: Option<String>,
) -> Result<()> {
    init_tracing();
    let (mut cfg, _) = load_cfg(config_path)?;
    if let Some(ws) = workspace {
        cfg.workspace = std::path::absolute(ws)?;
    }
    if let Some(host) = host {
        cfg.host = host;
    }
    if let Some(port) = port {
        cfg.port = port;
    }
    if let Some(token) = auth_token.or_else(|| std::env::var("AGENTBRIDGE_AUTH_TOKEN").ok()) {
        if !token.is_empty() {
            cfg.auth_token = Some(token);
        }
    }

    let final_allow_any_host = allow_any_host || cfg.allow_any_host;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(server::serve(cfg, final_allow_any_host))
}

fn cmd_status(config_path: Option<PathBuf>) -> Result<()> {
    let (cfg, path) = load_cfg(config_path)?;
    let ws = Workspace::open(
        &cfg.workspace,
        cfg.security.max_file_size,
        cfg.security.deny_sensitive_files,
    )?;
    let info = ws.info();
    let state = BridgeState::load(ws.root())?;

    println!("config     {}", path.display());
    println!("workspace  {}", info.workspace);
    println!(
        "project    {}",
        if info.project_type.is_empty() {
            "unknown".into()
        } else {
            info.project_type.join(", ")
        }
    );
    println!("mcp        {}", cfg.mcp_url());
    println!("git repo   {}", info.git_repository);
    if info.git_repository {
        match git::status(ws.root()) {
            Ok(st) => {
                println!(
                    "git        {} ({})",
                    st.branch,
                    if st.clean { "clean" } else { "dirty" }
                );
                if !st.changed_files.is_empty() {
                    println!("changed    {}", st.changed_files.join(", "));
                }
            }
            Err(err) => println!("git        {err}"),
        }
    }
    println!(
        "task       {}  iteration={}  state={}  lifecycle={}",
        state.task_id.as_deref().unwrap_or("(none)"),
        state.iteration,
        state.state,
        state.task_status.map(|s| s.as_str()).unwrap_or("none")
    );
    if let Some(executor) = &state.executor {
        println!("executor   {executor}");
    }
    if let Some(status) = &state.status {
        println!("status     {status}");
    }
    if let Some(code) = state.exit_code {
        println!("exit_code  {code}");
    }
    if let Some(summary) = &state.summary {
        println!("summary    {summary}");
    }
    if let Some(tests) = &state.tests {
        println!(
            "tests      {} ({}) exit={:?}",
            tests.command, tests.status, tests.exit_code
        );
    }
    Ok(())
}

fn cmd_doctor(config_path: Option<PathBuf>) -> Result<()> {
    let loaded = load_cfg(config_path).ok();
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

fn cmd_task(command: TaskCmd) -> Result<()> {
    match command {
        TaskCmd::Start {
            config,
            goal,
            tests,
            execute,
        } => {
            let (cfg, _) = load_cfg(config)?;
            let goal = goal.unwrap_or_else(|| "Implement the requested change.".into());
            let tests: Vec<String> = tests
                .map(|t| {
                    t.split(['\n', ';'])
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            if execute {
                return cmd_task_execute(cfg, goal, tests);
            }
            let mut state = BridgeState::load(&cfg.workspace)?;
            let task_id = new_task_id();
            let plan = agentbridge::C2cPlan::new(
                task_id.clone(),
                1,
                goal,
                vec!["Implement the goal in the current workspace.".into()],
                tests,
                "The goal is implemented and listed tests pass.".into(),
            )?;
            state.apply_plan(
                &plan,
                &cfg.workspace.display().to_string(),
                &cfg.executor.kind,
            );
            state.save(&cfg.workspace)?;
            state.write_c2c(
                &cfg.workspace,
                Some("Executor: implement this PLAN, run TESTS, then `agentbridge task executed` (or use task_start)."),
            )?;
            println!("task_id    {}", state.task_id.as_deref().unwrap_or(""));
            println!("iteration  {}", state.iteration);
            println!("state      {}", state.state);
            println!(
                "c2c        {}",
                config::current_c2c_path(&cfg.workspace).display()
            );
            println!();
            print!("{}", state.to_c2c(None).render());
            Ok(())
        }
        TaskCmd::Executed {
            config,
            task_id,
            iteration,
            status,
            tests,
            exit_code,
            changed_files,
            summary,
        } => {
            let (cfg, _) = load_cfg(config)?;
            let mut state = BridgeState::load(&cfg.workspace)?;
            if let Some(id) = task_id {
                state.task_id = Some(id);
            }
            if state.task_id.is_none() {
                state.task_id = Some(new_task_id());
            }
            state.iteration = iteration.unwrap_or(state.iteration.max(1));
            state.state = match status {
                ExecStatus::Blocked => C2cState::Blocked,
                _ => C2cState::Executed,
            };
            state.task_status = Some(match status {
                ExecStatus::Success => TaskStatus::Executed,
                ExecStatus::Failure => TaskStatus::Failed,
                ExecStatus::Blocked => TaskStatus::Blocked,
            });
            state.status = Some(status.as_str().to_string());
            state.finished_at = Some(Utc::now());
            state.exit_code = exit_code;
            if let Some(files) = changed_files {
                state.changed_files = files
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            } else if git::is_repository(&cfg.workspace) {
                if let Ok(st) = git::status(&cfg.workspace) {
                    let mut files = st.changed_files;
                    files.extend(st.untracked_files);
                    files.sort();
                    files.dedup();
                    state.changed_files = files;
                }
            }
            let command = tests.or_else(|| state.tests.as_ref().map(|t| t.command.clone()));
            if let Some(command) = command {
                let passed = exit_code.unwrap_or(1) == 0 && matches!(status, ExecStatus::Success);
                state.tests = Some(TestResult {
                    status: if passed {
                        "passed".into()
                    } else {
                        "failed".into()
                    },
                    command,
                    exit_code,
                    summary,
                    timestamp: Utc::now(),
                });
            }
            state.updated_at = Utc::now();
            state.save(&cfg.workspace)?;
            state.write_c2c(
                &cfg.workspace,
                Some("Please inspect the current workspace and git diff through MCP."),
            )?;
            println!("task_id    {}", state.task_id.as_deref().unwrap_or(""));
            println!("iteration  {}", state.iteration);
            println!("state      {}", state.state);
            println!("status     {}", state.status.as_deref().unwrap_or(""));
            if !state.changed_files.is_empty() {
                println!("changed    {}", state.changed_files.join(", "));
            }
            println!();
            print!("{}", state.to_c2c(None).render());
            Ok(())
        }
        TaskCmd::Status { config } => {
            let (cfg, _) = load_cfg(config)?;
            let state = BridgeState::load(&cfg.workspace)?;
            println!("{}", serde_json::to_string_pretty(&state)?);
            println!();
            print!("{}", state.to_c2c(None).render());
            Ok(())
        }
        TaskCmd::Cancel { config, task_id } => cmd_task_cancel(config, task_id),
    }
}

fn cmd_task_execute(cfg: Config, goal: String, tests: Vec<String>) -> Result<()> {
    let ws = Workspace::open(
        &cfg.workspace,
        cfg.security.max_file_size,
        cfg.security.deny_sensitive_files,
    )?;
    let cfg = std::sync::Arc::new(cfg);
    let ws = std::sync::Arc::new(ws);
    let runtime = TaskRuntime::new(ws, cfg)?;
    let plan = PlanInput {
        actions: vec!["Implement the goal in the current workspace.".into()],
        tests,
        success_criteria: "The goal is implemented and listed tests pass.".into(),
    };
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let state = runtime.start_task(goal, plan).await?;
        println!("task_id    {}", state.task_id.as_deref().unwrap_or(""));
        println!("status     running");
        let state = runtime.wait().await?;
        println!("status     {}", state.status.as_deref().unwrap_or(""));
        if let Some(code) = state.exit_code {
            println!("exit_code  {code}");
        }
        if !state.changed_files.is_empty() {
            println!("changed    {}", state.changed_files.join(", "));
        }
        if let Some(summary) = &state.summary {
            println!("summary    {summary}");
        }
        println!();
        print!("{}", state.to_c2c(None).render());
        if state.task_status == Some(TaskStatus::Failed) {
            bail!("executor failed");
        }
        Ok(())
    })
}

fn cmd_task_cancel(config: Option<PathBuf>, task_id: Option<String>) -> Result<()> {
    let (cfg, _) = load_cfg(config)?;
    let ws = Workspace::open(
        &cfg.workspace,
        cfg.security.max_file_size,
        cfg.security.deny_sensitive_files,
    )?;
    let cfg = std::sync::Arc::new(cfg);
    let ws = std::sync::Arc::new(ws);
    let runtime = TaskRuntime::new(ws, cfg)?;
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let state = runtime.cancel(task_id.as_deref()).await?;
        println!("task_id    {}", state.task_id.as_deref().unwrap_or(""));
        println!(
            "status     {}",
            state.status.as_deref().unwrap_or("cancelled")
        );
        Ok(())
    })
}
