use super::on_blocking_thread;
use deptide_core::error::AppResult;
use deptide_core::transfer::{
    self, ClipboardContents, ClipboardDownload, CopyResult, ExtractProgress, ReceiveClipboard,
    ReceiveFileContents, ReceivePlan, ReceiveRequest, ReceiveResult, ReceiveSelection,
    SystemClipboard, TransferPreview,
};
use deptide_core::workspace::open_with_config;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

#[tauri::command]
pub async fn preview_transfer(
    root: String,
    project_names: Vec<String>,
    extra_patterns: Vec<String>,
    disabled_patterns: Vec<String>,
) -> AppResult<TransferPreview> {
    on_blocking_thread(move || {
        let (workspace, config) = open_with_config(&root)?;
        transfer::preview(
            &workspace,
            &config,
            &project_names,
            &extra_patterns,
            &disabled_patterns,
        )
    })
    .await
}

#[tauri::command]
pub async fn copy_projects_to_clipboard(
    root: String,
    project_names: Vec<String>,
    extra_patterns: Vec<String>,
    disabled_patterns: Vec<String>,
) -> AppResult<CopyResult> {
    on_blocking_thread(move || {
        let (workspace, config) = open_with_config(&root)?;
        transfer::copy_to_clipboard(
            &workspace,
            &config,
            &project_names,
            &extra_patterns,
            &disabled_patterns,
        )
    })
    .await
}

#[tauri::command]
pub async fn inspect_clipboard(app: AppHandle, root: String) -> AppResult<ClipboardContents> {
    on_blocking_thread(move || {
        let (workspace, config) = open_with_config(&root)?;
        let known = transfer::known_projects(&workspace, &config);
        Ok(app
            .state::<ReceiveClipboard>()
            .inspect(&SystemClipboard, &known))
    })
    .await
}

#[tauri::command]
pub async fn download_clipboard(
    app: AppHandle,
    sequence: u32,
    on_progress: Channel<ExtractProgress>,
) -> AppResult<ClipboardDownload> {
    on_blocking_thread(move || {
        app.state::<ReceiveClipboard>()
            .download(&SystemClipboard, sequence, &mut |progress| {
                let _ = on_progress.send(progress);
            })
    })
    .await
}

#[tauri::command]
pub fn cancel_clipboard_download(receive: State<'_, ReceiveClipboard>) {
    receive.cancel_download();
}

#[tauri::command]
pub async fn analyze_receive(
    root: String,
    requests: Vec<ReceiveRequest>,
    extra_patterns: Vec<String>,
    disabled_patterns: Vec<String>,
) -> AppResult<ReceivePlan> {
    on_blocking_thread(move || {
        let (workspace, config) = open_with_config(&root)?;
        transfer::analyze_receive(
            &workspace,
            &config,
            &requests,
            &extra_patterns,
            &disabled_patterns,
        )
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

#[tauri::command]
pub async fn read_receive_file(
    root: String,
    source: String,
    target: String,
    relative: String,
) -> AppResult<ReceiveFileContents> {
    on_blocking_thread(move || {
        let (workspace, config) = open_with_config(&root)?;
        transfer::read_receive_file(&workspace, &config, &source, &target, &relative)
    })
    .await
}
