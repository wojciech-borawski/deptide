use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};
use crate::scan::MANIFEST_FILE_NAME;
use crate::util::text::slugify;

const LOCKFILE_NAME: &str = "package-lock.json";
const MAX_STORED_BACKUPS: usize = 20;

pub const BACKED_UP_FILES: [&str; 2] = [MANIFEST_FILE_NAME, LOCKFILE_NAME];

pub fn backup_project(
    backups_root: &Path,
    run_folder: &str,
    project_name: &str,
    project_directory: &Path,
) -> AppResult<Option<PathBuf>> {
    let target = backups_root
        .join(run_folder)
        .join(slugify(project_name).replace('.', "_"));
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

pub fn prune_backups(backups_root: &Path) {
    let Ok(entries) = fs::read_dir(backups_root) else {
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
