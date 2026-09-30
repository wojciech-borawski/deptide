use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::files::{
    collect_files, collect_received_files, copy_file, same_content, same_ignoring_whitespace,
    sha256_hex, to_posix, GIT_DIRECTORY,
};
use super::merge::{merge_lines, LineChunk};
use super::recycle::Recycler;
use crate::error::{AppError, AppResult};
use crate::scan::{read_project_at, MANIFEST_FILE_NAME};

const NODE_MODULES_DIRECTORY: &str = "node_modules";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileStatus {
    Added,
    Replaced,
    Whitespace,
    Identical,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiveFile {
    pub relative: String,
    pub status: FileStatus,
    /// Size of the received file, or of the target file for `removed`.
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiveProjectPlan {
    pub source: String,
    pub target: String,
    pub target_directory: String,
    pub files: Vec<ReceiveFile>,
    pub skipped: usize,
    pub added: usize,
    pub replaced: usize,
    pub whitespace: usize,
    pub identical: usize,
    pub removed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiveSelection {
    pub source: String,
    pub target: String,
    pub files: Vec<String>,
    /// Target files to move to the Recycle Bin; each must be absent from `source`.
    #[serde(default)]
    pub delete: Vec<String>,
    /// Files to rebuild from some chunks of the received file; a path here is
    /// never copied whole, even when `files` lists it too.
    #[serde(default)]
    pub merges: Vec<FileMerge>,
}

/// The chunks of one file's line diff to take from the received file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMerge {
    pub relative: String,
    /// `FileSide::Text::sha256` of the received file the chunks were computed from.
    pub received_sha256: String,
    /// `FileSide::Text::sha256` of the target file the chunks were computed from.
    pub local_sha256: String,
    /// How many chunks the diff had.
    pub total: usize,
    /// The chunks to take, in file order.
    pub chunks: Vec<LineChunk>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergedFile {
    pub relative: String,
    pub taken: usize,
    pub total: usize,
    /// The chunks that were taken.
    pub chunks: Vec<LineChunk>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiveProjectResult {
    pub target: String,
    pub target_directory: String,
    pub added: usize,
    pub replaced: usize,
    pub deleted: usize,
    /// Overwritten plus deleted files moved to the Recycle Bin.
    pub recycled: usize,
    pub bytes: u64,
    pub files: Vec<String>,
    pub deleted_files: Vec<String>,
    /// Entries of `files`, `delete` or `merges` that failed validation and were left alone.
    pub skipped: Vec<String>,
    /// Merges written; their paths are in `files` and counted in `replaced`.
    pub merged: Vec<MergedFile>,
    /// Merges left alone because either file is gone or no longer has the hash sent.
    pub stale: Vec<String>,
}

pub struct KnownProject {
    pub name: String,
    pub directory: PathBuf,
    pub package_name: Option<String>,
}

pub fn suggest_target(folder: &Path, known: &[KnownProject]) -> Option<String> {
    let folder_name = folder.file_name()?.to_string_lossy().to_string();
    let package_name = read_project_at(folder, folder).and_then(|project| project.package_name);
    suggest_target_for(&folder_name, package_name.as_deref(), known)
}

pub fn suggest_target_for(
    folder_name: &str,
    package_name: Option<&str>,
    known: &[KnownProject],
) -> Option<String> {
    known
        .iter()
        .find(|project| project.name.eq_ignore_ascii_case(folder_name))
        .or_else(|| {
            known.iter().find(|project| {
                project
                    .directory
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(folder_name))
            })
        })
        .or_else(|| {
            let wanted = package_name?;
            known
                .iter()
                .find(|project| project.package_name.as_deref() == Some(wanted))
        })
        .map(|project| project.name.clone())
}

pub fn analyze(
    source: &Path,
    target_name: &str,
    target_directory: &Path,
    patterns: &[String],
) -> ReceiveProjectPlan {
    let collection = collect_received_files(source, patterns);

    let mut files: Vec<ReceiveFile> = collection
        .files
        .iter()
        .map(|file| ReceiveFile {
            relative: to_posix(&file.relative),
            status: received_status(&file.absolute, &target_directory.join(&file.relative)),
            size: file.size,
        })
        .collect();
    files.extend(removed_files(source, target_directory, patterns));
    files.sort_by(|a, b| Path::new(&a.relative).cmp(Path::new(&b.relative)));

    let count = |status: FileStatus| files.iter().filter(|file| file.status == status).count();

    ReceiveProjectPlan {
        source: source.to_string_lossy().to_string(),
        target: target_name.to_string(),
        target_directory: target_directory.to_string_lossy().to_string(),
        added: count(FileStatus::Added),
        replaced: count(FileStatus::Replaced),
        whitespace: count(FileStatus::Whitespace),
        identical: count(FileStatus::Identical),
        removed: count(FileStatus::Removed),
        files,
        skipped: collection.skipped,
    }
}

fn received_status(received: &Path, destination: &Path) -> FileStatus {
    if !destination.exists() {
        FileStatus::Added
    } else if same_content(received, destination) {
        FileStatus::Identical
    } else if same_ignoring_whitespace(received, destination) {
        FileStatus::Whitespace
    } else {
        FileStatus::Replaced
    }
}

/// Target files the received folder does not have, judged with the same
/// ignore rules as the received files so build output and git-ignored files
/// are never offered for deletion.
fn removed_files(source: &Path, target_directory: &Path, patterns: &[String]) -> Vec<ReceiveFile> {
    if !target_directory.is_dir() {
        return Vec::new();
    }

    // A target without its own .gitignore would otherwise list node_modules.
    let mut target_patterns: Vec<String> = patterns.to_vec();
    target_patterns.push(format!("{NODE_MODULES_DIRECTORY}/"));

    collect_files(target_directory, &target_patterns, true)
        .files
        .into_iter()
        .filter(|file| !exists(&source.join(&file.relative)))
        .map(|file| ReceiveFile {
            relative: to_posix(&file.relative),
            status: FileStatus::Removed,
            size: file.size,
        })
        .collect()
}

struct PlannedCopy {
    relative: String,
    from: PathBuf,
    to: PathBuf,
    overwrites: bool,
}

struct PlannedDeletion {
    relative: String,
    path: PathBuf,
}

struct PlannedMerge {
    path: PathBuf,
    bytes: Vec<u8>,
    file: MergedFile,
}

pub fn apply(
    selection: &ReceiveSelection,
    target_directory: &Path,
    recycler: &dyn Recycler,
) -> AppResult<ReceiveProjectResult> {
    let source = Path::new(&selection.source);
    let mut skipped = Vec::new();
    let mut stale = Vec::new();
    let merge_paths: HashSet<String> = selection
        .merges
        .iter()
        .filter_map(|merge| safe_relative(&merge.relative))
        .map(|relative| to_posix(&relative))
        .collect();
    let copies = planned_copies(
        source,
        target_directory,
        &selection.files,
        &merge_paths,
        &mut skipped,
    );
    let deletions = planned_deletions(source, target_directory, &selection.delete, &mut skipped);
    let merges = planned_merges(
        source,
        target_directory,
        &selection.merges,
        &mut skipped,
        &mut stale,
    );

    let recycled: Vec<PathBuf> = copies
        .iter()
        .filter(|copy| copy.overwrites)
        .map(|copy| copy.to.clone())
        .chain(deletions.iter().map(|deletion| deletion.path.clone()))
        .chain(merges.iter().map(|merge| merge.path.clone()))
        .collect();
    if !recycled.is_empty() {
        recycler.recycle(&recycled).map_err(|error| {
            AppError::new(format!(
                "Could not move files of {} to the Recycle Bin (some may be there already), nothing was copied into it: {error}",
                selection.target
            ))
        })?;
    }

    let mut result = ReceiveProjectResult {
        target: selection.target.clone(),
        target_directory: target_directory.to_string_lossy().to_string(),
        added: 0,
        replaced: 0,
        deleted: deletions.len(),
        recycled: recycled.len(),
        bytes: 0,
        files: Vec::new(),
        deleted_files: deletions
            .iter()
            .map(|deletion| deletion.relative.clone())
            .collect(),
        skipped,
        merged: Vec::new(),
        stale,
    };

    for copy in &copies {
        result.bytes += copy_file(&copy.from, &copy.to).map_err(|error| {
            AppError::new(format!(
                "Copying {} into {} failed, the files it replaced are in the Recycle Bin: {error}",
                copy.relative, selection.target
            ))
        })?;
        if copy.overwrites {
            result.replaced += 1;
        } else {
            result.added += 1;
        }
        result.files.push(copy.relative.clone());
    }

    for merge in merges {
        fs::write(&merge.path, &merge.bytes).map_err(|error| {
            AppError::new(format!(
                "Writing the merged {} into {} failed, the file it replaced is in the Recycle Bin: {error}",
                merge.file.relative, selection.target
            ))
        })?;
        result.bytes += merge.bytes.len() as u64;
        result.replaced += 1;
        result.files.push(merge.file.relative.clone());
        result.merged.push(merge.file);
    }

    for deletion in &deletions {
        remove_emptied_folders(&deletion.path, target_directory);
    }

    Ok(result)
}

fn planned_copies(
    source: &Path,
    target_directory: &Path,
    entries: &[String],
    merged: &HashSet<String>,
    skipped: &mut Vec<String>,
) -> Vec<PlannedCopy> {
    let mut seen = HashSet::new();
    let mut copies = Vec::new();

    for entry in entries {
        let Some(relative) = safe_relative(entry) else {
            skipped.push(entry.clone());
            continue;
        };
        let posix = to_posix(&relative);
        if merged.contains(&posix) || !seen.insert(posix.clone()) {
            continue;
        }

        let from = source.join(&relative);
        let to = target_directory.join(&relative);
        if !from.is_file() || to.is_dir() || has_file_as_folder(target_directory, &relative) {
            skipped.push(entry.clone());
            continue;
        }

        copies.push(PlannedCopy {
            relative: posix,
            overwrites: to.is_file(),
            from,
            to,
        });
    }

    copies
}

/// Each merge whose files still hash as sent, with its output built in
/// memory; stale ones go to `stale`, invalid ones to `skipped`.
fn planned_merges(
    source: &Path,
    target_directory: &Path,
    entries: &[FileMerge],
    skipped: &mut Vec<String>,
    stale: &mut Vec<String>,
) -> Vec<PlannedMerge> {
    let mut seen = HashSet::new();
    let mut merges = Vec::new();

    for entry in entries {
        let Some(relative) = safe_relative(&entry.relative) else {
            skipped.push(entry.relative.clone());
            continue;
        };
        let posix = to_posix(&relative);
        if !seen.insert(posix.clone()) {
            skipped.push(entry.relative.clone());
            continue;
        }

        let path = target_directory.join(&relative);
        let (Some(local), Some(received)) =
            (fs::read(&path).ok(), fs::read(source.join(&relative)).ok())
        else {
            stale.push(entry.relative.clone());
            continue;
        };
        if sha256_hex(&local) != entry.local_sha256
            || sha256_hex(&received) != entry.received_sha256
        {
            stale.push(entry.relative.clone());
            continue;
        }

        let bytes = match merge_lines(&local, &received, &entry.chunks) {
            Ok(bytes) if entry.chunks.len() <= entry.total => bytes,
            _ => {
                skipped.push(entry.relative.clone());
                continue;
            }
        };
        merges.push(PlannedMerge {
            path,
            bytes,
            file: MergedFile {
                relative: posix,
                taken: entry.chunks.len(),
                total: entry.total,
                chunks: entry.chunks.clone(),
            },
        });
    }

    merges
}

fn planned_deletions(
    source: &Path,
    target_directory: &Path,
    entries: &[String],
    skipped: &mut Vec<String>,
) -> Vec<PlannedDeletion> {
    let root = fs::canonicalize(target_directory).ok();
    let mut seen = HashSet::new();
    let mut deletions = Vec::new();

    for entry in entries {
        let valid = safe_relative(entry).filter(|relative| {
            let path = target_directory.join(relative);
            // Equal only when no link, case difference or name alias sits below the root.
            let resolved_as_named = root.as_ref().is_some_and(|root| {
                fs::canonicalize(&path).is_ok_and(|canonical| canonical == root.join(relative))
            });
            resolved_as_named && path.is_file() && !exists(&source.join(relative))
        });

        match valid {
            Some(relative) => {
                let posix = to_posix(&relative);
                if seen.insert(posix.clone()) {
                    deletions.push(PlannedDeletion {
                        relative: posix,
                        path: target_directory.join(relative),
                    });
                }
            }
            None => skipped.push(entry.clone()),
        }
    }

    deletions
}

/// `raw` as a path of plain names, or `None` when it is empty, could point
/// outside the folder it is joined to, or goes through `.git`.
pub(super) fn safe_relative(raw: &str) -> Option<PathBuf> {
    let mut relative = PathBuf::new();

    for segment in raw.split(['/', '\\']) {
        let mut components = Path::new(segment).components();
        match (components.next(), components.next()) {
            (Some(Component::Normal(name)), None)
                if is_plain_name(segment) && !segment.eq_ignore_ascii_case(GIT_DIRECTORY) =>
            {
                relative.push(name)
            }
            _ => return None,
        }
    }

    Some(relative)
}

#[cfg(windows)]
fn is_plain_name(segment: &str) -> bool {
    // Windows reads `name:stream` as a data stream and drops trailing dots and spaces.
    !segment.contains(':') && !segment.ends_with(['.', ' '])
}

#[cfg(not(windows))]
fn is_plain_name(_segment: &str) -> bool {
    true
}

fn has_file_as_folder(target_directory: &Path, relative: &Path) -> bool {
    relative
        .ancestors()
        .skip(1)
        .filter(|folder| !folder.as_os_str().is_empty())
        .any(|folder| target_directory.join(folder).is_file())
}

fn exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn remove_emptied_folders(deleted: &Path, target_directory: &Path) {
    for folder in deleted.ancestors().skip(1) {
        if folder == target_directory || fs::remove_dir(folder).is_err() {
            break;
        }
    }
}

pub fn is_project_folder(path: &Path) -> bool {
    path.join(MANIFEST_FILE_NAME).is_file()
}
