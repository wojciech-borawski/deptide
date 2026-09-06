use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

#[cfg(windows)]
pub fn set_file_list(paths: &[PathBuf]) -> AppResult<()> {
    use clipboard_win::{formats, Clipboard, Setter};

    let entries: Vec<String> = paths
        .iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect();

    let _guard = Clipboard::new_attempts(10)
        .map_err(|error| AppError::with_context("Clipboard is busy", error))?;

    formats::FileList
        .write_clipboard(&entries[..])
        .map_err(|error| AppError::with_context("Clipboard write failed", error))
}

#[cfg(windows)]
pub fn file_list() -> Vec<PathBuf> {
    use clipboard_win::{formats, get_clipboard};

    get_clipboard::<Vec<String>, _>(formats::FileList)
        .map(|entries| entries.into_iter().map(PathBuf::from).collect())
        .unwrap_or_default()
}

#[cfg(not(windows))]
pub fn set_file_list(paths: &[PathBuf]) -> AppResult<()> {
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|error| AppError::with_context("Clipboard is unavailable", error))?;

    clipboard
        .set()
        .file_list(paths)
        .map_err(|error| AppError::with_context("Clipboard write failed", error))
}

#[cfg(not(windows))]
pub fn file_list() -> Vec<PathBuf> {
    arboard::Clipboard::new()
        .ok()
        .and_then(|mut clipboard| clipboard.get().file_list().ok())
        .unwrap_or_default()
}

pub fn directories(paths: &[PathBuf]) -> Vec<PathBuf> {
    paths
        .iter()
        .filter(|path| Path::new(path).is_dir())
        .cloned()
        .collect()
}
