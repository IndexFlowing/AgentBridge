mod commands;
mod dto;

use std::sync::Mutex;
use tauri::Manager;

use commands::{
    executor::{available_executors, choose_executor_directory, choose_executor_file, delete_executor, executors, save_executor, test_executor},
    project::{choose_project_directory, delete_project, save_project},
    proxy::{proxy, save_proxy, test_proxy_connection},
    system::{connection, dashboard, save_connection, settings, start_gateway, stop_gateway, GatewayState},
    task::cancel_task,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(GatewayState(Mutex::new(None)))
        .setup(|app| {
            // 👈 桌面端启动时：自动读取配置（端口如 8030），如端口未被占用则自动拉起 Core Gateway
            let state = app.state::<GatewayState>();
            let _ = start_gateway(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            dashboard,
            start_gateway, // 👈 一键启动
            stop_gateway,  // 👈 一键停止
            connection,
            settings,
            proxy,
            available_executors,
            executors,
            choose_executor_file,
            choose_executor_directory,
            save_executor,
            delete_executor,
            test_executor,
            choose_project_directory,
            save_project,
            delete_project,
            save_proxy,
            test_proxy_connection,
            save_connection,
            cancel_task,
        ])
        .run(tauri::generate_context!())
        .expect("error while running AgentBridge desktop");
}