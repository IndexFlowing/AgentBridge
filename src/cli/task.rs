// src/cli/task.rs
use anyhow::{bail, Context, Result};
use chrono::Utc;
use clap::{Subcommand, ValueEnum};
use std::sync::Arc;

use agentbridge::config;
use agentbridge::git;
use agentbridge::projects::ProjectHub;
use agentbridge::protocol::{C2cPlan, C2cState};
use agentbridge::state::{new_task_id, TaskStatus, TestResult};
use agentbridge::storage::Storage;
use agentbridge::task::PlanInput;

#[derive(Subcommand)]
pub enum TaskCmd {
    Start {
        #[arg(long)]
        goal: Option<String>,
        #[arg(long)]
        tests: Option<String>,
        #[arg(long)]
        executor: Option<String>,
        #[arg(long)]
        execute: bool,
    },
    Executed {
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
    Status,
    Cancel {
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
    let (cfg, _config_path) = config::load_or_create_user_config()?;
    let storage = Storage::init()?;
    let projects = storage.load_projects()?;
    let default_project = projects
        .first()
        .context("No projects mounted. Please mount one via Web UI first.")?;
    let project_name = default_project.name.clone();
    let workspace_path = default_project.path.clone();

    match command {
        TaskCmd::Start {
            goal,
            tests,
            executor,
            execute,
        } => {
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
                return execute_task(storage, cfg, goal, tests, executor);
            }

            let mut state = storage.load_task_state(&project_name)?;
            let task_id = new_task_id();
            let executor_kind = executor.as_deref().unwrap_or(&cfg.executor.kind);
            let plan = C2cPlan::new(
                task_id.clone(),
                1,
                goal,
                vec!["Implement the goal in the workspace.".into()],
                tests,
                "Goal implemented and tests pass.".into(),
            )?;

            state.apply_plan(&plan, &workspace_path.display().to_string(), executor_kind);
            storage.save_task_state(&project_name, &state)?;
            state.write_c2c(
                &workspace_path,
                Some("Executor: implement this PLAN, run TESTS, then report."),
            )?;

            println!("task_id    {}", state.task_id.as_deref().unwrap_or(""));
            println!("iteration  {}", state.iteration);
            println!("state      {}", state.state);
            Ok(())
        }
        TaskCmd::Executed {
            task_id,
            iteration,
            status,
            tests,
            exit_code,
            changed_files,
            summary,
        } => {
            let mut state = storage.load_task_state(&project_name)?;
            if let Some(id) = task_id {
                state.task_id = Some(id);
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
            } else if git::is_repository(&workspace_path) {
                if let Ok(st) = git::status(&workspace_path) {
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
            storage.save_task_state(&project_name, &state)?;
            state.write_c2c(&workspace_path, Some("Please inspect through MCP."))?;
            println!("status     {}", state.status.as_deref().unwrap_or(""));
            Ok(())
        }
        TaskCmd::Status => {
            let state = storage.load_task_state(&project_name)?;
            println!("{}", serde_json::to_string_pretty(&state)?);
            Ok(())
        }
        TaskCmd::Cancel { task_id } => cancel_task(storage, cfg, task_id),
    }
}

fn execute_task(
    storage: Storage,
    cfg: agentbridge::config::Config,
    goal: String,
    tests: Vec<String>,
    executor_override: Option<String>,
) -> Result<()> {
    let storage_arc = Arc::new(storage);
    let hub = ProjectHub::new(Arc::new(cfg), storage_arc)?;
    let project = hub
        .get(hub.default_name())
        .context("default project not found")?;
    let runtime = project.runtime.clone();

    let plan = PlanInput {
        actions: vec!["Implement the goal.".into()],
        tests,
        success_criteria: "Tests pass.".into(),
    };
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let _ = runtime
            .start_task(goal, plan, executor_override.as_deref())
            .await?;
        println!("status     running");
        let state = runtime.wait().await?;
        println!("status     {}", state.status.as_deref().unwrap_or(""));
        if state.task_status == Some(TaskStatus::Failed) {
            bail!("executor failed");
        }
        Ok(())
    })
}

fn cancel_task(
    storage: Storage,
    cfg: agentbridge::config::Config,
    task_id: Option<String>,
) -> Result<()> {
    let storage_arc = Arc::new(storage);
    let hub = ProjectHub::new(Arc::new(cfg), storage_arc)?;
    let project = hub
        .get(hub.default_name())
        .context("default project not found")?;
    let runtime = project.runtime.clone();
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let state = runtime.cancel(task_id.as_deref()).await?;
        println!(
            "status     {}",
            state.status.as_deref().unwrap_or("cancelled")
        );
        Ok(())
    })
}
