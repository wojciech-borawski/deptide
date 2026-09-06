use std::path::Path;

use super::on_blocking_thread;
use crate::app::error_log;
use crate::app::update::{self, UpdateInfo};
use deptide_core::error::AppResult;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub identifier: &'static str,
}

#[tauri::command]
pub async fn check_for_update(url: String) -> AppResult<UpdateInfo> {
    on_blocking_thread(move || update::check(&url)).await
}

#[tauri::command]
pub fn save_text_file(path: String, content: String) -> AppResult<()> {
    std::fs::write(&path, content)?;
    Ok(())
}

#[tauri::command]
pub fn log_client_error(source: String, message: String) {
    error_log::record(&source, &message);
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        name: "Deptide",
        version: update::CURRENT_VERSION,
        identifier: "dev.deptide.app",
    }
}

#[tauri::command]
pub fn startup_workspace() -> Option<String> {
    std::env::args()
        .nth(1)
        .filter(|argument| Path::new(argument).is_dir())
}

#[tauri::command]
pub fn error_log_path() -> Option<String> {
    error_log::path().map(|path| path.to_string_lossy().to_string())
}
