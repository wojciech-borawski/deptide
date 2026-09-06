use std::path::PathBuf;

use tauri::{AppHandle, Manager};

use crate::app::dto::{ProjectView, WorkspaceSnapshot};
use deptide_core::domain::{Settings, UpdateConfig};
use deptide_core::error::{AppError, AppResult};
use deptide_core::execution::list_summaries;
use deptide_core::scan::{find_duplicate_groups, read_project_at, MANIFEST_FILE_NAME};
use deptide_core::workspace::{
    delete_run, list_recent, list_runs, load_config, load_settings, open_with_config,
    remember_recent, save_config as write_config, save_settings as write_settings,
    suggest_run_name, RecentWorkspace, Workspace,
};

fn app_config_dir(app: &AppHandle) -> AppResult<PathBuf> {
    app.path()
        .app_config_dir()
        .map_err(|error| AppError::new(error.to_string()))
}

fn project_views(workspace: &Workspace, config: &UpdateConfig) -> Vec<ProjectView> {
    let duplicates = find_duplicate_groups(&config.projects, workspace);

    config
        .projects
        .iter()
        .map(|project| {
            let absolute = workspace.resolve_project(&project.path);
            let duplicate_of = duplicates
                .iter()
                .filter(|group| group.entries.contains(&project.name))
                .flat_map(|group| group.entries.iter())
                .filter(|name| *name != &project.name)
                .cloned()
                .collect();

            let kind = read_project_at(&absolute, &absolute)
                .map(|found| found.kind)
                .unwrap_or_default();

            ProjectView {
                name: project.name.clone(),
                path: project.path.clone(),
                absolute_path: absolute.to_string_lossy().to_string(),
                exists: absolute.join(MANIFEST_FILE_NAME).is_file(),
                kind,
                skip: project.skip,
                packages: project.packages.clone(),
                install_args: project.install_args.clone(),
                duplicate_of,
            }
        })
        .collect()
}

pub(super) fn build_snapshot(
    app: &AppHandle,
    workspace: &Workspace,
) -> AppResult<WorkspaceSnapshot> {
    let config = load_config(workspace)?;
    let settings = load_settings(workspace);

    Ok(WorkspaceSnapshot {
        root: workspace.root().to_string_lossy().to_string(),
        config_file: workspace.config_file().to_string_lossy().to_string(),
        projects: project_views(workspace, &config),
        config,
        settings,
        saved_runs: list_runs(workspace),
        history: list_summaries(&workspace.logs_directory()),
        recent: list_recent(&app_config_dir(app)?),
    })
}

#[tauri::command]
pub fn list_recent_workspaces(app: AppHandle) -> AppResult<Vec<RecentWorkspace>> {
    Ok(list_recent(&app_config_dir(&app)?))
}

#[tauri::command]
pub fn open_workspace(app: AppHandle, path: String) -> AppResult<WorkspaceSnapshot> {
    let workspace = Workspace::open(&path)?;
    remember_recent(&app_config_dir(&app)?, &workspace.root().to_string_lossy())?;

    build_snapshot(&app, &workspace)
}

#[tauri::command]
pub fn save_settings(root: String, settings: Settings) -> AppResult<Settings> {
    let workspace = Workspace::open(&root)?;
    write_settings(&workspace, &settings)
}

#[tauri::command]
pub fn save_config(
    app: AppHandle,
    root: String,
    config: UpdateConfig,
) -> AppResult<WorkspaceSnapshot> {
    let workspace = Workspace::open(&root)?;
    write_config(&workspace, &config)?;
    build_snapshot(&app, &workspace)
}

#[tauri::command]
pub fn delete_saved_run(root: String, name: String) -> AppResult<()> {
    let (workspace, _) = open_with_config(&root)?;
    delete_run(&workspace, &name)
}

#[tauri::command]
pub fn suggest_run_label(package_names: Vec<String>) -> String {
    suggest_run_name(&package_names)
}
