use std::path::{Path, PathBuf};

use super::virtual_files::VirtualListing;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardFiles {
    Paths(Vec<PathBuf>),
    Virtual(VirtualListing),
    Empty,
    Busy,
    Unreadable(String),
}

#[cfg(windows)]
pub fn read_clipboard_files() -> ClipboardFiles {
    use clipboard_win::{formats, get_clipboard, is_format_avail};

    if !is_format_avail(formats::CF_HDROP) {
        return super::virtual_files::read_listing();
    }
    match get_clipboard::<Vec<String>, _>(formats::FileList) {
        Ok(entries) => paths_or_empty(entries.into_iter().map(PathBuf::from).collect()),
        Err(_) => ClipboardFiles::Busy,
    }
}

#[cfg(windows)]
pub fn clipboard_sequence() -> Option<u32> {
    Some(super::virtual_files::sequence_number()).filter(|sequence| *sequence != 0)
}

#[cfg(not(windows))]
pub fn clipboard_sequence() -> Option<u32> {
    None
}

#[cfg(not(windows))]
pub fn read_clipboard_files() -> ClipboardFiles {
    paths_or_empty(file_list())
}

fn paths_or_empty(paths: Vec<PathBuf>) -> ClipboardFiles {
    if paths.is_empty() {
        ClipboardFiles::Empty
    } else {
        ClipboardFiles::Paths(paths)
    }
}

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
