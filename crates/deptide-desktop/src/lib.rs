pub mod app;

use tauri::{Manager, WindowEvent};

use app::commands;
use app::state::AppState;
use app::{error_log, tray};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .manage(AppState::default())
        .setup(|app| {
            if let Ok(log_directory) = app.path().app_log_dir() {
                error_log::install(&log_directory);
            }
            if let Err(error) = tray::install(app.handle()) {
                error_log::record("tray", &error.to_string());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::Destroyed = event {
                window.state::<AppState>().abort_all();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_recent_workspaces,
            commands::open_workspace,
            commands::save_settings,
            commands::save_config,
            commands::scan_projects,
            commands::apply_scan,
            commands::inspect_projects,
            commands::start_run,
            commands::start_command_run,
            commands::abort_run,
            commands::abort_job,
            commands::restore_project,
            commands::check_for_update,
            commands::get_run_snapshot,
            commands::delete_saved_run,
            commands::suggest_run_label,
            commands::render_run_report,
            commands::save_text_file,
            commands::log_client_error,
            commands::error_log_path,
            commands::startup_workspace,
            commands::app_info,
            commands::preview_transfer,
            commands::copy_projects_to_clipboard,
            commands::inspect_clipboard,
            commands::analyze_receive,
            commands::apply_receive,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Deptide");
}
