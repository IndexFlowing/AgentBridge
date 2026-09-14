// src/infra/notification.rs
//! Cross-platform native desktop notification service.
//!
//! Dispatches OS toasts (Windows Action Center, macOS Notification Center,
//! Linux libnotify) asynchronously without blocking task execution.

use notify_rust::{Notification, Timeout};

use crate::state::TaskStatus;

/// Send an asynchronous native OS desktop notification.
///
/// This call spawns an isolated background worker thread to ensure failures
/// or desktop environment timeouts never block the async runtime or core tasks.
pub fn send_task_notification(
    project_name: &str,
    goal: &str,
    status: TaskStatus,
    changed_files_count: usize,
    test_summary: Option<&str>,
) {
    let project = project_name.to_string();
    let goal = goal.to_string();
    let test_summary = test_summary.map(ToString::to_string);

    std::thread::spawn(move || {
        let title = match status {
            TaskStatus::Executed | TaskStatus::Done => "✦ AgentBridge 任务完成",
            TaskStatus::Cancelled => "✦ AgentBridge 任务已取消",
            _ => "✦ AgentBridge 任务失败",
        };

        let mut body = format!("[{project}] 目标: \"{goal}\"\n变更了 {changed_files_count} 个文件");
        if let Some(ref tests) = test_summary {
            body.push_str(&format!(" | 测试: {tests}"));
        }

        let _ = Notification::new()
            .appname("AgentBridge")
            .summary(title)
            .body(&body)
            .timeout(Timeout::Milliseconds(5000))
            .show();
    });
}
