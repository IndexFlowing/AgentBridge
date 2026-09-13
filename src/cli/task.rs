use std::path::PathBuf;
use std::sync::Arc;
use anyhow::{bail, Context, Result};
use chrono::Utc;
use clap::{Subcommand, ValueEnum};

use agentbridge::config::{self, Config};
use agentbridge::git;
use agentbridge::projects::ProjectHub;
use agentbridge::protocol::{C2cPlan, C2cState};
use agentbridge::state::{new_task_id, BridgeState, TaskStatus, TestResult};
use agentbridge::task::PlanInput;

#[derive(Subcommand)]
pub enum TaskCmd {
    /// Create a new task and write a PLAN. Use --execute to start OpenCode and wait.
    Start {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        goal: Option<String>,
        #[arg(long)]
        tests: Option<String>,
        #[arg(long)]
        executor: Option<String>,
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
pub enum ExecStatus {
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

pub fn run(command: TaskCmd) -> Result<()> {
    match command {
        TaskCmd::Start { config, goal, tests, executor, execute } => {
            let (cfg, config_path) = config::find_config(config.as_deref())?;
            let goal = goal.unwrap_or_else(|| "Implement the requested change.".into());
            let tests: Vec<String> = tests
                .map(|t| t.split(['\n', ';']).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
                .unwrap_or_default();
            if execute {
                return execute_task(cfg, config_path, goal, tests, executor);
            }
            let mut state = BridgeState::load(&cfg.workspace)?;
            let task_id = new_task_id();
            let executor_kind = executor.as_deref().unwrap_or(&cfg.executor.kind);
            let plan = C2cPlan::new(
                task_id.clone(),
                1,
                goal,
                vec!["Implement the goal in the current workspace.".into()],
                tests,
                "The goal is implemented and listed tests pass.".into(),
            )?;
            state.apply_plan(&plan, &cfg.workspace.display().to_string(), executor_kind);
            state.save(&cfg.workspace)?;
            state.write_c2c(&cfg.workspace, Some("Executor: implement this PLAN, run TESTS, then `agentbridge task executed`."))?;
            println!("task_id    {}", state.task_id.as_deref().unwrap_or(""));
            println!("iteration  {}", state.iteration);
            println!("state      {}", state.state);
            println!("executor   {}", state.executor.as_deref().unwrap_or(""));
            println!("c2c        {}", config::current_c2c_path(&cfg.workspace).display());
            println!();
            print!("{}", state.to_c2c(None).render());
            Ok(())
        }
        TaskCmd::Executed { config, task_id, iteration, status, tests, exit_code, changed_files, summary } => {
            let (cfg, _) = config::find_config(config.as_deref())?;
            let mut state = BridgeState::load(&cfg.workspace)?;
            if let Some(id) = task_id { state.task_id = Some(id); }
            if state.task_id.is_none() { state.task_id = Some(new_task_id()); }
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
                state.changed_files = files.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
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
                    status: if passed { "passed".into() } else { "failed".into() },
                    command,
                    exit_code,
                    summary,
                    timestamp: Utc::now(),
                });
            }
            state.updated_at = Utc::now();
            state.save(&cfg.workspace)?;
            state.write_c2c(&cfg.workspace, Some("Please inspect the current workspace and git diff through MCP."))?;
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
            let (cfg, _) = config::find_config(config.as_deref())?;
            let state = BridgeState::load(&cfg.workspace)?;
            println!("{}", serde_json::to_string_pretty(&state)?);
            println!();
            print!("{}", state.to_c2c(None).render());
            Ok(())
        }
        TaskCmd::Cancel { config, task_id } => cancel_task(config, task_id),
    }
}

fn execute_task(cfg: Config, config_path: PathBuf, goal: String, tests: Vec<String>, executor_override: Option<String>) -> Result<()> {
    let hub = ProjectHub::open_with_path(
        vec![agentbridge::projects::ProjectEntry {
            id: String::new(),
            name: "default".into(),
            path: cfg.workspace.clone(),
            description: "Default workspace".into(),
            readonly: false,
            executor: agentbridge::projects::default_project_executor(),
        }],
        Some("default".into()),
        Arc::new(cfg.clone()),
        config_path,
    )?;
    let project = hub.get("default").context("default project not found")?;
    let runtime = project.runtime.clone();

    let plan = PlanInput {
        actions: vec!["Implement the goal in the current workspace.".into()],
        tests,
        success_criteria: "The goal is implemented and listed tests pass.".into(),
    };
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let state = runtime.start_task(goal, plan, executor_override.as_deref()).await?;
        println!("task_id    {}", state.task_id.as_deref().unwrap_or(""));
        println!("executor   {}", state.executor.as_deref().unwrap_or(""));
        println!("status     running");
        let state = runtime.wait().await?;
        println!("status     {}", state.status.as_deref().unwrap_or(""));
        if let Some(code) = state.exit_code { println!("exit_code  {code}"); }
        if !state.changed_files.is_empty() { println!("changed    {}", state.changed_files.join(", ")); }
        if let Some(summary) = &state.summary { println!("summary    {summary}"); }
        println!();
        print!("{}", state.to_c2c(None).render());
        if state.task_status == Some(TaskStatus::Failed) {
            bail!("executor failed");
        }
        Ok(())
    })
}

fn cancel_task(config: Option<PathBuf>, task_id: Option<String>) -> Result<()> {
    let (cfg, config_path) = config::find_config(config.as_deref())?;
    let hub = ProjectHub::open_with_path(
        vec![agentbridge::projects::ProjectEntry {
            id: String::new(),
            name: "default".into(),
            path: cfg.workspace.clone(),
            description: "Default workspace".into(),
            readonly: false,
            executor: agentbridge::projects::default_project_executor(),
        }],
        Some("default".into()),
        Arc::new(cfg.clone()),
        config_path,
    )?;
    let project = hub.get("default").context("default project not found")?;
    let runtime = project.runtime.clone();
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let state = runtime.cancel(task_id.as_deref()).await?;
        println!("task_id    {}", state.task_id.as_deref().unwrap_or(""));
        println!("status     {}", state.status.as_deref().unwrap_or("cancelled"));
        Ok(())
    })
}