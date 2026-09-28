#[cfg(windows)]
mod ole;

use std::fs;
use std::io;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::scan::MANIFEST_FILE_NAME;

#[cfg(windows)]
use ole as platform;
#[cfg(windows)]
pub(super) use ole::{read_listing, sequence_number};
#[cfg(not(windows))]
use unsupported as platform;

const RESERVED_DEVICE_NAMES: [&str; 6] = ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"];
const FORBIDDEN_NAME_CHARACTERS: [char; 7] = ['<', '>', ':', '"', '|', '?', '*'];

/// Files the clipboard offers without a path. Reads stop once the clipboard
/// sequence number differs from `sequence`; `rejected` counts refused names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualListing {
    pub sequence: u32,
    pub entries: Vec<VirtualEntry>,
    pub rejected: usize,
}

/// `index` is the position in the clipboard's descriptor array, rejected
/// descriptors included, and selects the contents of the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualEntry {
    pub index: u32,
    pub relative: PathBuf,
    pub is_directory: bool,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualFolder {
    pub name: String,
    pub files: usize,
    pub bytes: u64,
    pub has_manifest: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractProgress {
    pub files_done: usize,
    pub files_total: usize,
    pub bytes_done: u64,
    pub bytes_total: Option<u64>,
}

pub fn sanitize_relative_path(name: &str) -> Option<PathBuf> {
    if name.starts_with(['/', '\\']) {
        return None;
    }

    let segments: Vec<&str> = name
        .split(['/', '\\'])
        .filter(|segment| !segment.is_empty())
        .collect();
    if segments.is_empty() || !segments.iter().all(|segment| is_safe_name(segment)) {
        return None;
    }

    Some(segments.into_iter().collect())
}

fn is_safe_name(segment: &str) -> bool {
    !segment.ends_with(['.', ' '])
        && !segment
            .chars()
            .any(|character| character < ' ' || FORBIDDEN_NAME_CHARACTERS.contains(&character))
        && !is_reserved_device_name(segment)
}

fn is_reserved_device_name(segment: &str) -> bool {
    let base = segment
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end_matches(' ')
        .to_ascii_uppercase();
    if RESERVED_DEVICE_NAMES.contains(&base.as_str()) {
        return true;
    }

    let port = base
        .strip_prefix("COM")
        .or_else(|| base.strip_prefix("LPT"));
    let mut number = port.unwrap_or_default().chars();
    matches!(
        (number.next(), number.next()),
        (Some('0'..='9' | '\u{b9}' | '\u{b2}' | '\u{b3}'), None)
    )
}

pub fn top_level_folders(listing: &VirtualListing) -> Vec<VirtualFolder> {
    let mut folders: Vec<VirtualFolder> = Vec::new();

    for entry in &listing.entries {
        let mut components = entry.relative.components();
        let Some(top) = components.next() else {
            continue;
        };
        let inside = components.as_path();
        if inside.as_os_str().is_empty() && !entry.is_directory {
            continue;
        }

        let name = top.as_os_str().to_string_lossy().to_string();
        let position = match folders.iter().position(|folder| folder.name == name) {
            Some(position) => position,
            None => {
                folders.push(VirtualFolder {
                    name,
                    files: 0,
                    bytes: 0,
                    has_manifest: false,
                });
                folders.len() - 1
            }
        };

        if !entry.is_directory {
            let folder = &mut folders[position];
            folder.files += 1;
            folder.bytes += entry.size.unwrap_or(0);
            folder.has_manifest |= inside == Path::new(MANIFEST_FILE_NAME);
        }
    }

    folders
}

pub fn read_virtual_file(
    listing: &VirtualListing,
    relative: &Path,
    limit: u64,
) -> AppResult<Vec<u8>> {
    let entry = listing
        .entries
        .iter()
        .find(|entry| !entry.is_directory && entry.relative == relative)
        .ok_or_else(|| {
            AppError::new(format!(
                "{} is not a file on the clipboard",
                relative.display()
            ))
        })?;
    if entry.size.is_some_and(|size| size > limit) {
        return Err(larger_than(relative, limit));
    }

    platform::read_file(listing.sequence, entry, limit)
}

fn larger_than(relative: &Path, limit: u64) -> AppError {
    AppError::new(format!(
        "{} is larger than {limit} bytes",
        relative.display()
    ))
}

/// Writes `listing` into an empty `destination`, returning its top-level folders; errors remove
/// `destination`. `progress` runs at least every 200 ms; `Break` stops the extraction.
pub fn extract_virtual(
    listing: &VirtualListing,
    destination: &Path,
    progress: &mut dyn FnMut(ExtractProgress) -> ControlFlow<()>,
) -> AppResult<Vec<PathBuf>> {
    ensure_empty_destination(destination)?;

    if let Err(error) = platform::extract(listing, destination, progress) {
        let _ = fs::remove_dir_all(destination);
        return Err(error);
    }

    Ok(top_level_folders(listing)
        .into_iter()
        .map(|folder| destination.join(folder.name))
        .collect())
}

fn ensure_empty_destination(destination: &Path) -> AppResult<()> {
    let is_empty = match fs::read_dir(destination) {
        Ok(mut entries) => entries.next().is_none(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => true,
        Err(error) => return Err(error.into()),
    };

    if is_empty {
        Ok(())
    } else {
        Err(AppError::new(format!(
            "{} is not empty",
            destination.display()
        )))
    }
}

#[cfg(not(windows))]
mod unsupported {
    use std::ops::ControlFlow;
    use std::path::Path;

    use super::{ExtractProgress, VirtualEntry, VirtualListing};
    use crate::error::{AppError, AppResult};

    const UNSUPPORTED: &str = "Files without a path on the clipboard are read on Windows only";

    pub fn read_file(_sequence: u32, _entry: &VirtualEntry, _limit: u64) -> AppResult<Vec<u8>> {
        Err(AppError::new(UNSUPPORTED))
    }

    pub fn extract(
        _listing: &VirtualListing,
        _destination: &Path,
        _progress: &mut dyn FnMut(ExtractProgress) -> ControlFlow<()>,
    ) -> AppResult<()> {
        Err(AppError::new(UNSUPPORTED))
    }
}
