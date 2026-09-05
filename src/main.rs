use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{bail, Result};
use chrono::Utc;
use clap::{Parser, Subcommand, ValueEnum};

use agentbridge::config::{self, Config, ExecutorDefinition, ProxyKind};
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
        /// Single workspace directory (treated as project `default`)
        #[arg(value_name = "DIR")]
        dir: Option<PathBuf>,
        /// Config file (defaults to ./.agentbridge.toml or ~/.agentbridge/config.toml)
        #[arg(long)]
        config: Option<PathBuf>,
        /// Multi-project workspace file (`agentbridge.config.json`)
        #[arg(long)]
        workspaces: Option<PathBuf>,
        /// Override workspace path (single-project mode)
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
        /// Static Authorization: Bearer <token> accepted on /mcp (in addition to OAuth)
        #[arg(long)]
        auth_token: Option<String>,
        /// Disable OAuth/Bearer checks (localhost / development only)
        #[arg(long)]
        no_auth: bool,
        /// Alias for --no-auth
        #[arg(long)]
        dev: bool,
        /// Pre-registered OAuth client_id (otherwise clients use dynamic registration)
        #[arg(long)]
        client_id: Option<String>,
        /// Pre-registered OAuth client_secret
        #[arg(long)]
        client_secret: Option<String>,
        /// Password shown on /oauth/authorize (or AGENTBRIDGE_ADMIN_PASSWORD)
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
        command: TaskCmd,
    },
    /// Manage mounted projects in agentbridge.config.json
    Project {
        #[command(subcommand)]
        command: ProjectCmd,
    },
    /// Discover and manage local executor installations
    Executor {
        #[command(subcommand)]
        command: ExecutorCmd,
    },
    /// Show, configure, or test the executor proxy
    Proxy {
        #[command(subcommand)]
        command: ProxyCmd,
    },
    /// Open the tray console (start/stop MCP, edit projects and OAuth)
    Tray,
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

#[derive(Subcommand)]
enum ProjectCmd {
    /// List projects from a workspace file
    List {
        #[arg(long, default_value = "agentbridge.config.json")]
        workspaces: PathBuf,
    },
    /// Add a project to a workspace file
    Add {
        name: String,
        path: PathBuf,
        #[arg(long, default_value = "agentbridge.config.json")]
        workspaces: PathBuf,
        #[arg(long, default_value = "")]
        description: String,
        #[arg(long)]
        readonly: bool,
        #[arg(long)]
        default: bool,
    },
    /// Remove a project from a workspace file
    Remove {
        name: String,
        #[arg(long, default_value = "agentbridge.config.json")]
        workspaces: PathBuf,
    },
}

#[derive(Subcommand)]
enum ExecutorCmd {
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

#[derive(Subcommand)]
enum ProxyCmd {
    /// Show proxy settings with credentials redacted
    Show { #[arg(long)] config: Option<PathBuf> },
    /// Update proxy settings in the AgentBridge config
    Set {
        #[arg(long)] config: Option<PathBuf>,
        #[arg(long)] kind: Option<ProxyKindArg>,
        #[arg(long)] host: Option<String>,
        #[arg(long)] port: Option<u16>,
        #[arg(long)] username: Option<String>,
        #[arg(long)] password: Option<String>,
        #[arg(long)] disable: bool,
    },
    /// Connect to example.com through the configured proxy
    Test { #[arg(long)] config: Option<PathBuf> },
}

#[derive(Clone, Copy, ValueEnum)]
enum ProxyKindArg { Http, Https, Socks5 }

impl From<ProxyKindArg> for ProxyKind {
    fn from(value: ProxyKindArg) -> Self {
        match value {
            ProxyKindArg::Http => Self::Http,
            ProxyKindArg::Https => Self::Https,
            ProxyKindArg::Socks5 => Self::Socks5,
        }
    }
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
            dir,
            config,
            workspaces,
            workspace,
            host,
            port,
            allow_any_host,
            auth_token,
            no_auth,
            dev,
            client_id,
            client_secret,
            admin_password,
        } => cmd_serve(ServeArgs {
            dir,
            config,
            workspaces,
            workspace,
            host,
            port,
            allow_any_host,
            auth_token,
            no_auth: no_auth || dev,
            client_id,
            client_secret,
            admin_password,
        }),
        Commands::Status { config } => cmd_status(config),
        Commands::Doctor { config } => cmd_doctor(config),
        Commands::Task { command } => cmd_task(command),
        Commands::Project { command } => cmd_project(command),
        Commands::Executor { command } => cmd_executor(command),
        Commands::Proxy { command } => cmd_proxy(command),
        Commands::Tray => cmd_tray(),
    }
}

fn cmd_project(command: ProjectCmd) -> Result<()> {
    match command {
        ProjectCmd::List { workspaces } => {
            let (projects, default) = agentbridge::projects::load_workspaces_file(&workspaces)?;
            for project in projects {
                println!(
                    "{}\t{}\t{}{}",
                    project.name,
                    project.path.display(),
                    if project.readonly { "readonly" } else { "writable" },
                    if default.as_deref() == Some(project.name.as_str()) { "\tdefault" } else { "" }
                );
            }
            Ok(())
        }
        ProjectCmd::Add { name, path, workspaces, description, readonly, default } => {
            let path = std::path::absolute(path)?;
            if !path.is_dir() { bail!("project path is not a directory: {}", path.display()); }
            let (mut projects, current_default) = if workspaces.is_file() {
                agentbridge::projects::load_workspaces_file(&workspaces)?
            } else { (Vec::new(), None) };
            let name = agentbridge::projects::validate_project_name(&name)?;
            if projects.iter().any(|p| p.name == name) { bail!("project `{name}` already exists"); }
            projects.push(agentbridge::projects::ProjectEntry {
                id: String::new(), name: name.clone(), path, description, readonly,
                executor: "opencode".into(),
            });
            let default = if default || current_default.is_none() { Some(name.clone()) } else { current_default };
            agentbridge::projects::save_workspaces_file(&workspaces, &projects, default)?;
            println!("added project `{name}` to {}", workspaces.display());
            Ok(())
        }
        ProjectCmd::Remove { name, workspaces } => {
            let (mut projects, default) = agentbridge::projects::load_workspaces_file(&workspaces)?;
            let before = projects.len();
            projects.retain(|project| !project.name.eq_ignore_ascii_case(&name));
            if projects.len() == before { bail!("project `{name}` was not found"); }
            if projects.is_empty() { bail!("cannot remove the last project"); }
            let default = if default.as_deref().is_some_and(|d| d.eq_ignore_ascii_case(&name)) {
                Some(projects[0].name.clone())
            } else { default };
            agentbridge::projects::save_workspaces_file(&workspaces, &projects, default)?;
            println!("removed project `{name}` from {}", workspaces.display());
            Ok(())
        }
    }
}

fn cmd_executor(command: ExecutorCmd) -> Result<()> {
    match command {
        ExecutorCmd::List { config } => {
            let config_path = config.unwrap_or_else(config::project_config_path);
            let registry = config::load_executor_registry(&config_path)?;
            for (definition, discovered) in agentbridge::executor::executor_definitions_with_discovery(&registry.executors) {
                let availability = agentbridge::executor::scan_executor(&definition);
                println!("{}\t{}\t{}\t{}", definition.id, definition.display_name, definition.kind,
                    if discovered { format!("discovered:{}", availability_status(&availability)) } else { format!("saved:{}", availability_status(&availability)) });
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
            let entries = agentbridge::executor::executor_definitions_with_discovery(&registry.executors);
            let mut found = false;
            for (definition, _) in entries {
                if id.as_deref().is_some_and(|wanted| wanted != definition.id) { continue; }
                found = true;
                let result = agentbridge::executor::scan_executor(&definition);
                println!("{}\t{}\t{}", definition.id, definition.display_name, availability_status(&result));
            }
            if !found { bail!("no executor matched the requested ID"); }
            Ok(())
        }
    }
}

fn availability_status(availability: &agentbridge::executor::ExecutorAvailability) -> &'static str {
    match availability.status {
        agentbridge::executor::ExecutorAvailabilityStatus::Available => "available",
        agentbridge::executor::ExecutorAvailabilityStatus::NotFound => "not_found",
        agentbridge::executor::ExecutorAvailabilityStatus::NotExecutable => "not_executable",
        agentbridge::executor::ExecutorAvailabilityStatus::VersionProbeFailed => "version_probe_failed",
    }
}

fn cmd_proxy(command: ProxyCmd) -> Result<()> {
    match command {
        ProxyCmd::Show { config } => {
            let (cfg, path) = load_cfg(config)?;
            println!("config  {}", path.display());
            println!("enabled {}", cfg.proxy.enabled);
            println!("kind    {:?}", cfg.proxy.kind);
            println!("host    {}", cfg.proxy.host);
            println!("port    {}", cfg.proxy.port);
            println!("auth    {}", if cfg.proxy.username.is_some() { "configured" } else { "none" });
            Ok(())
        }
        ProxyCmd::Set { config, kind, host, port, username, password, disable } => {
            let (mut cfg, path) = load_cfg(config)?;
            if let Some(kind) = kind { cfg.proxy.kind = kind.into(); }
            if let Some(host) = host { cfg.proxy.host = host; }
            if let Some(port) = port { cfg.proxy.port = port; }
            if let Some(username) = username { cfg.proxy.username = Some(username); }
            if let Some(password) = password { cfg.proxy.password = Some(password); }
            cfg.proxy.enabled = !disable;
            cfg.proxy.validate()?;
            cfg.save_to_path(&path)?;
            println!("saved proxy settings to {}", path.display());
            Ok(())
        }
        ProxyCmd::Test { config } => {
            let (cfg, _) = load_cfg(config)?;
            let rt = tokio::runtime::Runtime::new()?;
            println!("{}", rt.block_on(agentbridge::executor::test_proxy(&cfg.proxy))?);
            Ok(())
        }
    }
}

fn cmd_tray() -> Result<()> {
    init_tracing();
    agentbridge::ui::run()
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
    println!("Edit `.agentbridge.toml` to set admin_password (empty = unused).");
    println!("Start the Brain endpoint with:");
    println!("  agentbridge serve");
    println!();
    println!("Default MCP URL: {}", cfg.mcp_url());
    Ok(())
}

fn load_cfg(explicit: Option<PathBuf>) -> Result<(Config, PathBuf)> {
    config::find_config(explicit.as_deref())
}

struct ServeArgs {
    dir: Option<PathBuf>,
    config: Option<PathBuf>,
    workspaces: Option<PathBuf>,
    workspace: Option<PathBuf>,
    host: Option<String>,
    port: Option<u16>,
    allow_any_host: bool,
    auth_token: Option<String>,
    no_auth: bool,
    client_id: Option<String>,
    client_secret: Option<String>,
    admin_password: Option<String>,
}

fn cmd_serve(args: ServeArgs) -> Result<()> {
    init_tracing();

    let mut cfg = if let Some(path) = args.config.as_deref() {
        Config::load_from_path(path)?
    } else if args.dir.is_some() || args.workspaces.is_some() {
        match config::find_config(None) {
            Ok((c, _)) => c,
            Err(_) => {
                let path = args
                    .dir
                    .clone()
                    .or(args.workspace.clone())
                    .unwrap_or_else(|| std::env::current_dir().expect("cwd"));
                Config::new(std::path::absolute(path)?)
            }
        }
    } else {
        load_cfg(None)?.0
    };

    if let Some(host) = args.host {
        cfg.host = host;
    }
    if let Some(port) = args.port {
        cfg.port = port;
    }

    let (entries, default_name) = agentbridge::projects::discover(
        args.workspaces.as_deref(),
        args.dir.as_deref(),
        args.workspace.as_deref(),
        &cfg.workspace,
    )?;
    if let Some(first) = entries.first() {
        cfg.workspace = first.path.clone();
        if let Some(name) = &default_name {
            if let Some(found) = entries.iter().find(|e| &e.name == name) {
                cfg.workspace = found.path.clone();
            }
        }
    }

    if let Some(token) = config::first_nonempty(
        args.auth_token,
        "AGENTBRIDGE_AUTH_TOKEN",
        cfg.auth_token.clone(),
    ) {
        cfg.auth_token = Some(token);
    }

    let cfg_arc_source = cfg.clone();
    let hub = agentbridge::projects::ProjectHub::open(
        entries,
        default_name,
        std::sync::Arc::new(cfg_arc_source),
    )?;

    let no_auth = args.no_auth
        || cfg.no_auth
        || std::env::var("AGENTBRIDGE_NO_AUTH").is_ok_and(|v| {
            matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes")
        });

    let options = server::ServeOptions {
        allow_any_host: args.allow_any_host || cfg.allow_any_host,
        no_auth,
        client_id: config::first_nonempty(
            args.client_id,
            "AGENTBRIDGE_CLIENT_ID",
            cfg.client_id.clone(),
        ),
        client_secret: config::first_nonempty(
            args.client_secret,
            "AGENTBRIDGE_CLIENT_SECRET",
            cfg.client_secret.clone(),
        ),
        admin_password: config::first_nonempty(
            args.admin_password,
            "AGENTBRIDGE_ADMIN_PASSWORD",
            cfg.admin_password.clone(),
        ),
    };

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(server::serve(cfg, hub, options))
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_help_is_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn cli_parses_core_management_commands() {
        assert!(matches!(
            Cli::try_parse_from(["agentbridge", "project", "list"]).unwrap().command,
            Commands::Project { command: ProjectCmd::List { .. } }
        ));
        assert!(matches!(
            Cli::try_parse_from(["agentbridge", "executor", "test"]).unwrap().command,
            Commands::Executor { command: ExecutorCmd::Test { .. } }
        ));
        assert!(matches!(
            Cli::try_parse_from(["agentbridge", "proxy", "set", "--disable"]).unwrap().command,
            Commands::Proxy { command: ProxyCmd::Set { disable: true, .. } }
        ));
    }
}
