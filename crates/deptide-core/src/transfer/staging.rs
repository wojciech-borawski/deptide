use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::Utc;

use crate::error::{AppError, AppResult};
use crate::util::time::file_stamp;

const STAGING_ROOT: &str = "deptide-transfer";
const RECEIVE_ROOT: &str = "deptide-received";
const MAX_KEPT_FOLDERS: usize = 5;
const MAX_FOLDERS_PER_STAMP: u32 = 99;

pub fn create_staging_directory() -> AppResult<PathBuf> {
    let root = std::env::temp_dir().join(STAGING_ROOT);
    fs::create_dir_all(&root)?;
    prune_old_folders(&root, None);

    let folder = root.join(file_stamp(Utc::now()));
    fs::create_dir_all(&folder)?;
    Ok(folder)
}

pub fn create_receive_directory() -> AppResult<PathBuf> {
    let root = std::env::temp_dir().join(RECEIVE_ROOT);
    fs::create_dir_all(&root)?;

    let folder = create_new_folder(&root, &file_stamp(Utc::now()))?;
    prune_old_folders(&root, Some(&folder));
    Ok(folder)
}

fn create_new_folder(root: &Path, stamp: &str) -> AppResult<PathBuf> {
    for attempt in 1..=MAX_FOLDERS_PER_STAMP {
        let name = if attempt == 1 {
            stamp.to_string()
        } else {
            format!("{stamp}-{attempt:02}")
        };
        let folder = root.join(name);
        match fs::create_dir(&folder) {
            Ok(()) => return Ok(folder),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }

    Err(AppError::new(format!(
        "No free folder name for {stamp} in {}",
        root.display()
    )))
}

fn prune_old_folders(root: &Path, spared: Option<&Path>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };

    let mut folders: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && Some(path.as_path()) != spared)
        .collect();
    folders.sort();
    folders.reverse();

    for folder in folders.into_iter().skip(MAX_KEPT_FOLDERS - 1) {
        let _ = fs::remove_dir_all(folder);
    }
}
