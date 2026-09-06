use super::on_blocking_thread;
use deptide_core::error::AppResult;
use deptide_core::transfer::{
    self, ClipboardContents, CopyResult, ReceivePlan, ReceiveRequest, ReceiveResult,
    ReceiveSelection, TransferPreview,
};
use deptide_core::workspace::open_with_config;

#[tauri::command]
pub async fn preview_transfer(
    root: String,
    project_names: Vec<String>,
    extra_patterns: Vec<String>,
) -> AppResult<TransferPreview> {
    on_blocking_thread(move || {
        let (workspace, config) = open_with_config(&root)?;
        transfer::preview(&workspace, &config, &project_names, &extra_patterns)
    })
    .await
}

#[tauri::command]
pub async fn copy_projects_to_clipboard(
    root: String,
    project_names: Vec<String>,
    extra_patterns: Vec<String>,
) -> AppResult<CopyResult> {
    on_blocking_thread(move || {
        let (workspace, config) = open_with_config(&root)?;
        transfer::copy_to_clipboard(&workspace, &config, &project_names, &extra_patterns)
    })
    .await
}

#[tauri::command]
pub async fn inspect_clipboard(root: String) -> AppResult<ClipboardContents> {
    on_blocking_thread(move || {
        let (workspace, config) = open_with_config(&root)?;
        Ok(transfer::inspect_clipboard(&workspace, &config))
    })
    .await
}

#[tauri::command]
pub async fn analyze_receive(
    root: String,
    requests: Vec<ReceiveRequest>,
    extra_patterns: Vec<String>,
) -> AppResult<ReceivePlan> {
    on_blocking_thread(move || {
        let (workspace, config) = open_with_config(&root)?;
        transfer::analyze_receive(&workspace, &config, &requests, &extra_patterns)
    })
    .await
}

#[tauri::command]
pub async fn apply_receive(
    root: String,
    selections: Vec<ReceiveSelection>,
) -> AppResult<ReceiveResult> {
    on_blocking_thread(move || {
        let (workspace, config) = open_with_config(&root)?;
        transfer::apply_receive(&workspace, &config, &selections)
    })
    .await
}
