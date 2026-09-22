use std::path::{Path, PathBuf};

use super::on_blocking_thread;
use crate::app::dto::{ProjectInspection, ScanResult, ScannedProject};
use deptide_core::domain::{DetectedProject, Settings};
use deptide_core::error::{AppError, AppResult};
use deptide_core::scan::{
    apply_scan_selection, collect_dependency_candidates, detect_projects, read_project_at,
    DetectionOptions, ScanSelectionOutcome,
};
use deptide_core::workspace::{
    load_settings, normalize_directory, open_with_config, save_config as write_config, Workspace,
};

fn detection_options(settings: &Settings, workspace: &Workspace) -> DetectionOptions {
    DetectionOptions::new(settings.scan_depth, &settings.extra_ignored_directories)
        .excluding(workspace.internal_directories())
}

fn projects_root(settings: &Settings) -> AppResult<PathBuf> {
    let root = Path::new(&settings.projects_root);

    if settings.projects_root.is_empty() || !root.is_dir() {
        return Err(AppError::new(
            "Set the projects root folder in the settings before scanning",
        ));
    }

    Ok(root.to_path_buf())
}

fn configured_name_by_directory(
    workspace: &Workspace,
    config: &deptide_core::domain::UpdateConfig,
) -> Vec<(String, String)> {
    config
        .projects
        .iter()
        .map(|project| {
            (
                normalize_directory(&workspace.resolve_project(&project.path)),
                project.name.clone(),
            )
        })
        .collect()
}

#[tauri::command]
pub async fn scan_projects(root: String) -> AppResult<ScanResult> {
    let (workspace, config) = open_with_config(&root)?;
    let settings = load_settings(&workspace);
    let projects_root = projects_root(&settings)?;
    let options = detection_options(&settings, &workspace);

    let scan_root = projects_root.clone();
    let detected = on_blocking_thread(move || Ok(detect_projects(&scan_root, &options))).await?;

    let configured = configured_name_by_directory(&workspace, &config);

    let projects = detected
        .into_iter()
        .map(|project| {
            let directory = normalize_directory(Path::new(&project.directory));
            let configured_name = configured
                .iter()
                .find(|(known, _)| known == &directory)
                .map(|(_, name)| name.clone());

            ScannedProject {
                config_path: workspace.to_config_path(Path::new(&project.directory)),
                configured_name,
                detected: project,
            }
        })
        .collect();

    Ok(ScanResult {
        projects_root: projects_root.to_string_lossy().to_string(),
        depth: settings.scan_depth,
        projects,
    })
}

#[tauri::command]
pub fn apply_scan(
    root: String,
    offered: Vec<DetectedProject>,
    selected_directories: Vec<String>,
) -> AppResult<ScanSelectionOutcome> {
    let (workspace, config) = open_with_config(&root)?;

    let outcome = apply_scan_selection(&workspace, &config, &offered, &selected_directories);
    write_config(&workspace, &outcome.config)?;

    Ok(outcome)
}

#[tauri::command]
pub async fn inspect_projects(
    root: String,
    project_names: Vec<String>,
) -> AppResult<ProjectInspection> {
    let (workspace, config) = open_with_config(&root)?;
    let settings = load_settings(&workspace);

    let scan_root = projects_root(&settings).ok();
    let relative_root = scan_root
        .clone()
        .unwrap_or_else(|| workspace.root().to_path_buf());
    let options = detection_options(&settings, &workspace);
    let branch_pattern = settings.branch_suffix_pattern.clone();

    let selected: Vec<(PathBuf, String)> = config
        .projects
        .iter()
        .filter(|project| project_names.contains(&project.name))
        .map(|project| {
            (
                workspace.resolve_project(&project.path),
                project.name.clone(),
            )
        })
        .collect();

    on_blocking_thread(move || {
        let detected = scan_root
            .map(|scan_root| detect_projects(&scan_root, &options))
            .unwrap_or_default();

        let mut consumers = Vec::new();
        let mut unreadable = Vec::new();

        for (directory, name) in selected {
            let wanted = normalize_directory(&directory);
            let known = detected
                .iter()
                .find(|project| normalize_directory(Path::new(&project.directory)) == wanted);

            match known
                .cloned()
                .or_else(|| read_project_at(&directory, &relative_root))
            {
                Some(project) => consumers.push(project),
                None => unreadable.push(name),
            }
        }

        let mut all = detected;
        all.extend(consumers.iter().cloned());

        Ok(ProjectInspection {
            candidates: collect_dependency_candidates(&all, &consumers, &branch_pattern),
            unreadable,
        })
    })
    .await
}
