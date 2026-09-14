use anyhow::{bail, Result};
use clap::{Subcommand, ValueEnum};
use std::sync::Arc;

use crate::config;
use crate::core::AppCore;
use crate::models::{CancelTaskRequest, StartTaskRequest};
use crate::protocol::C2cState;
use crate::state::TaskStatus;
use crate::task::{ExecutedInput, PlanInput};

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
    let cfg = Arc::new(cfg);
    let core = AppCore::bootstrap(cfg.clone())?;
    let project_name = core.hub.default_name();

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
                return execute_task(&core, goal, tests, executor);
            }

            let executor_kind = executor.as_deref().unwrap_or(&cfg.executor.kind);
            let state = core
                .tasks
                .plan_task(&project_name, goal, tests, executor_kind)?;

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
            let changed_files = changed_files.map(|files| {
                files
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            });
            let input = ExecutedInput {
                task_id,
                iteration,
                c2c_state: match status {
                    ExecStatus::Blocked => C2cState::Blocked,
                    _ => C2cState::Executed,
                },
                task_status: match status {
                    ExecStatus::Success => TaskStatus::Executed,
                    ExecStatus::Failure => TaskStatus::Failed,
                    ExecStatus::Blocked => TaskStatus::Blocked,
                },
                status: status.as_str().to_string(),
                exit_code,
                changed_files,
                tests_command: tests,
                test_summary: summary,
            };
            let state = core.tasks.record_executed(&project_name, input)?;
            println!("status     {}", state.status.as_deref().unwrap_or(""));
            Ok(())
        }
        TaskCmd::Status => {
            let state = core.tasks.task_state(&project_name)?;
            println!("{}", serde_json::to_string_pretty(&state)?);
            Ok(())
        }
        TaskCmd::Cancel { task_id } => cancel_task(&core, project_name, task_id),
    }
}

fn execute_task(
    core: &AppCore,
    goal: String,
    tests: Vec<String>,
    executor_override: Option<String>,
) -> Result<()> {
    let project_name = core.hub.default_name();
    let plan = PlanInput {
        actions: vec!["Implement the goal.".into()],
        tests,
        success_criteria: "Tests pass.".into(),
    };
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let req = StartTaskRequest {
            project_name: project_name.clone(),
            goal,
            plan,
            skills: Vec::new(),
            executor: executor_override,
            continue_task_id: None,
        };
        let _ = core.tasks.start_task(req).await?;
        println!("status     running");
        let state = core.tasks.wait(&project_name).await?;
        println!("status     {}", state.status.as_deref().unwrap_or(""));
        if state.task_status == Some(TaskStatus::Failed) {
            bail!("executor failed");
        }
        Ok(())
    })
}

fn cancel_task(core: &AppCore, project_name: String, task_id: Option<String>) -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let req = CancelTaskRequest {
            project_name,
            task_id,
        };
        let state = core.tasks.cancel_task(req).await?;
        println!(
            "status     {}",
            state.status.as_deref().unwrap_or("cancelled")
        );
        Ok(())
    })
}
