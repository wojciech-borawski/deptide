use std::fs;
use std::path::{Component, Path, PathBuf};

use chrono::{DateTime, Utc};

use super::logger::{list_summaries, stamped_run_name};
use super::project_lock::{lock_project, restore_owner};
use crate::domain::{BackedUpProject, ProjectRestore, RestoreOutcome, RunBackup, RunSummary};
use crate::error::{AppError, AppResult};
use crate::scan::MANIFEST_FILE_NAME;
use crate::util::text::slugify;
use crate::workspace::Workspace;

const LOCKFILE_NAME: &str = "package-lock.json";
const MAX_STORED_BACKUPS: usize = 20;

pub const BACKED_UP_FILES: [&str; 2] = [MANIFEST_FILE_NAME, LOCKFILE_NAME];

fn backup_subfolder(project_name: &str) -> String {
    slugify(project_name).replace('.', "_")
}

pub fn backup_project(
    backups_root: &Path,
    run_folder: &str,
    project_name: &str,
    project_directory: &Path,
) -> AppResult<Option<PathBuf>> {
    let target = backups_root
        .join(run_folder)
        .join(backup_subfolder(project_name));
    let mut copied = false;

    for file_name in BACKED_UP_FILES {
        let source = project_directory.join(file_name);
        if !source.is_file() {
            continue;
        }

        fs::create_dir_all(&target)?;
        fs::copy(&source, target.join(file_name))?;
        copied = true;
    }

    Ok(copied.then_some(target))
}

pub fn restore_project(
    backup_directory: &Path,
    project_directory: &Path,
) -> AppResult<Vec<String>> {
    if !backup_directory.is_dir() {
        return Err(AppError::new(format!(
            "No backup found at {}",
            backup_directory.display()
        )));
    }

    let mut restored = Vec::new();

    for file_name in BACKED_UP_FILES {
        let source = backup_directory.join(file_name);
        if !source.is_file() {
            continue;
        }

        fs::copy(&source, project_directory.join(file_name))?;
        restored.push(file_name.to_string());
    }

    if restored.is_empty() {
        return Err(AppError::new("The backup folder holds no files to restore"));
    }

    Ok(restored)
}

pub fn restore_project_locked(
    backup_directory: &Path,
    project_directory: &Path,
    locks_directory: &Path,
) -> AppResult<Vec<String>> {
    let _lock = lock_project(locks_directory, project_directory, &restore_owner())
        .map_err(|error| AppError::new(format!("Not restored: {}", error.message())))?;
    restore_project(backup_directory, project_directory)
}

pub fn prune_backups(backups_root: &Path) {
    keep_newest_run_folders(backups_root);
}

pub(super) fn keep_newest_run_folders(root: &Path) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };

    let mut folders: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();

    folders.sort();
    folders.reverse();

    for folder in folders.into_iter().skip(MAX_STORED_BACKUPS) {
        let _ = fs::remove_dir_all(folder);
    }
}

pub fn run_name(log_file: &str) -> String {
    let file_name = Path::new(log_file)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    file_name
        .strip_suffix(".log")
        .unwrap_or(&file_name)
        .to_string()
}

fn backed_up_projects(run_directory: &Path, summary: &RunSummary) -> Vec<BackedUpProject> {
    let subfolders: Vec<String> = summary
        .projects
        .iter()
        .map(|project| backup_subfolder(&project.name))
        .collect();

    summary
        .projects
        .iter()
        .zip(&subfolders)
        .filter(|(_, subfolder)| subfolders.iter().filter(|other| other == subfolder).count() == 1)
        .filter_map(|(project, subfolder)| {
            let directory = run_directory.join(subfolder);
            let files: Vec<String> = BACKED_UP_FILES
                .iter()
                .filter(|file_name| directory.join(file_name).is_file())
                .map(|file_name| file_name.to_string())
                .collect();
            (!files.is_empty()).then(|| BackedUpProject {
                name: project.name.clone(),
                directory: project.directory.clone(),
                files,
            })
        })
        .collect()
}

fn is_single_folder_name(name: &str) -> bool {
    let mut components = Path::new(name).components();
    matches!(
        (components.next(), components.next()),
        (Some(Component::Normal(_)), None)
    )
}

fn legacy_folder(summary: &RunSummary) -> Option<String> {
    if summary.backup_folder.is_some() || summary.dry_run {
        return None;
    }
    let started_at = DateTime::parse_from_rfc3339(&summary.started_at)
        .ok()?
        .with_timezone(&Utc);
    Some(stamped_run_name(started_at, &summary.label))
}

fn claimed_folder(summary: &RunSummary) -> Option<String> {
    summary
        .backup_folder
        .clone()
        .or_else(|| legacy_folder(summary))
}

fn run_folder(backups_root: &Path, summary: &RunSummary, history: &[RunSummary]) -> Option<String> {
    if summary.dry_run {
        return None;
    }
    if let Some(folder) = &summary.backup_folder {
        return Some(folder.clone());
    }

    let folder = legacy_folder(summary)?;
    let claimants = history
        .iter()
        .filter(|other| claimed_folder(other).as_deref() == Some(folder.as_str()))
        .count();
    (claimants == 1 && backups_root.join(&folder).is_dir()).then_some(folder)
}

fn find_run_backup(
    backups_root: &Path,
    summary: &RunSummary,
    history: &[RunSummary],
) -> Option<RunBackup> {
    let log_file = summary.log_file.clone()?;
    let folder = run_folder(backups_root, summary, history)
        .filter(|folder| is_single_folder_name(folder))?;
    let projects = backed_up_projects(&backups_root.join(&folder), summary);

    (!projects.is_empty()).then(|| RunBackup {
        run: run_name(&log_file),
        log_file,
        folder,
        projects,
    })
}

pub fn list_run_backups(workspace: &Workspace) -> Vec<RunBackup> {
    let backups_root = workspace.backups_directory();
    let history = list_summaries(&workspace.logs_directory());
    history
        .iter()
        .filter_map(|summary| find_run_backup(&backups_root, summary, &history))
        .collect()
}

pub struct RestoreRequest {
    pub run: String,
    pub projects: Vec<String>,
}

fn find_summary<'a>(history: &'a [RunSummary], run: &str) -> AppResult<&'a RunSummary> {
    let wanted = run_name(run);
    history
        .iter()
        .find(|summary| summary.log_file.as_deref().map(run_name).as_deref() == Some(&wanted))
        .ok_or_else(|| AppError::new(format!("No run named {wanted} in the workspace history")))
}

fn failed_restore(name: &str, error: String) -> ProjectRestore {
    ProjectRestore {
        name: name.to_string(),
        outcome: RestoreOutcome::Failed { error },
    }
}

fn restore_backed_up_project(
    run_directory: &Path,
    project: &BackedUpProject,
    locks_directory: &Path,
) -> ProjectRestore {
    let outcome = restore_project_locked(
        &run_directory.join(backup_subfolder(&project.name)),
        Path::new(&project.directory),
        locks_directory,
    );

    match outcome {
        Ok(files) => ProjectRestore {
            name: project.name.clone(),
            outcome: RestoreOutcome::Restored { files },
        },
        Err(error) => failed_restore(&project.name, error.to_string()),
    }
}

pub fn restore_run(
    workspace: &Workspace,
    request: &RestoreRequest,
) -> AppResult<Vec<ProjectRestore>> {
    let history = list_summaries(&workspace.logs_directory());
    let summary = find_summary(&history, &request.run)?;
    let backups_root = workspace.backups_directory();
    let backup = find_run_backup(&backups_root, summary, &history).ok_or_else(|| {
        AppError::new(format!(
            "Run {} has no backups left to restore",
            run_name(&request.run)
        ))
    })?;
    let run_directory = backups_root.join(&backup.folder);
    let locks_directory = workspace.locks_directory();
    let restore = |project| restore_backed_up_project(&run_directory, project, &locks_directory);

    if request.projects.is_empty() {
        return Ok(backup.projects.iter().map(restore).collect());
    }

    Ok(request
        .projects
        .iter()
        .map(
            |name| match backup.projects.iter().find(|project| &project.name == name) {
                Some(project) => restore(project),
                None => failed_restore(name, format!("This run holds no backup of {name}")),
            },
        )
        .collect())
}
