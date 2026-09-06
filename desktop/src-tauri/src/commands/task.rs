use crate::commands::{hub, load};
use crate::commands::system::task_data;
use crate::dto::TaskData;

#[tauri::command]
pub async fn cancel_task(project_name: String, task_id: Option<String>) -> Result<TaskData, String> {
    let (cfg, config_path) = load()?;
    let hub = hub(&cfg, &config_path)?;
    let project = hub.get(&project_name).ok_or_else(|| "project not found".to_string())?;
    let runtime = project.runtime.clone();
    runtime.cancel(task_id.as_deref()).await.map_err(|e| e.to_string())?;
    task_data(&project.name, &project.workspace)
}