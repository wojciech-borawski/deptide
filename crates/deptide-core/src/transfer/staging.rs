use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;

use crate::error::AppResult;
use crate::util::time::file_stamp;

const STAGING_ROOT: &str = "deptide-transfer";
const MAX_STAGED: usize = 5;

pub fn create_staging_directory() -> AppResult<PathBuf> {
    let root = std::env::temp_dir().join(STAGING_ROOT);
    fs::create_dir_all(&root)?;
    prune_old_staging_folders(&root);

    let folder = root.join(file_stamp(Utc::now()));
    fs::create_dir_all(&folder)?;
    Ok(folder)
}

fn prune_old_staging_folders(root: &Path) {
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

    for folder in folders.into_iter().skip(MAX_STAGED - 1) {
        let _ = fs::remove_dir_all(folder);
    }
}
