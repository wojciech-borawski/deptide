use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::files::{collect_files, copy_file, same_content, to_posix, Collection};
use crate::error::AppResult;
use crate::scan::{read_project_at, MANIFEST_FILE_NAME};

const NODE_MODULES_DIRECTORY: &str = "node_modules";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileStatus {
    Added,
    Replaced,
    Identical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiveFile {
    pub relative: String,
    pub status: FileStatus,
    pub size: u64,
}

/// A file that exists in the target project but not in the received folder:
/// it may have been deleted on the other machine and is a candidate for
/// removal after the merge. Deptide never deletes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetOnlyFile {
    pub relative: String,
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
    pub identical: usize,
    #[serde(default)]
    pub only_in_target: Vec<TargetOnlyFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiveSelection {
    pub source: String,
    pub target: String,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiveProjectResult {
    pub target: String,
    pub target_directory: String,
    pub added: usize,
    pub replaced: usize,
    pub bytes: u64,
    pub files: Vec<String>,
}

pub struct KnownProject {
    pub name: String,
    pub directory: PathBuf,
    pub package_name: Option<String>,
}

pub fn suggest_target(folder: &Path, known: &[KnownProject]) -> Option<String> {
    let folder_name = folder.file_name()?.to_string_lossy().to_string();
    let package_name = read_project_at(folder, folder).and_then(|project| project.package_name);

    known
        .iter()
        .find(|project| project.name.eq_ignore_ascii_case(&folder_name))
        .or_else(|| {
            known.iter().find(|project| {
                project
                    .directory
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(&folder_name))
            })
        })
        .or_else(|| {
            let wanted = package_name.as_deref()?;
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
    let collection = collect_files(source, patterns, true);

    let files: Vec<ReceiveFile> = collection
        .files
        .iter()
        .map(|file| {
            let destination = target_directory.join(&file.relative);
            let status = if !destination.exists() {
                FileStatus::Added
            } else if same_content(&file.absolute, &destination) {
                FileStatus::Identical
            } else {
                FileStatus::Replaced
            };

            ReceiveFile {
                relative: to_posix(&file.relative),
                status,
                size: file.size,
            }
        })
        .collect();

    let count = |status: FileStatus| files.iter().filter(|file| file.status == status).count();

    ReceiveProjectPlan {
        source: source.to_string_lossy().to_string(),
        target: target_name.to_string(),
        target_directory: target_directory.to_string_lossy().to_string(),
        added: count(FileStatus::Added),
        replaced: count(FileStatus::Replaced),
        identical: count(FileStatus::Identical),
        only_in_target: files_only_in_target(&collection, target_directory, patterns),
        files,
        skipped: collection.skipped,
    }
}

/// Files the target project has that the received folder does not, judged
/// with the same ignore rules as the received files so that build output and
/// git-ignored files do not show up as candidates.
fn files_only_in_target(
    received: &Collection,
    target_directory: &Path,
    patterns: &[String],
) -> Vec<TargetOnlyFile> {
    if !target_directory.is_dir() {
        return Vec::new();
    }

    let received_paths: HashSet<&Path> = received
        .files
        .iter()
        .map(|file| file.relative.as_path())
        .collect();

    // A target without its own .gitignore would otherwise list node_modules.
    let mut target_patterns: Vec<String> = patterns.to_vec();
    target_patterns.push(format!("{NODE_MODULES_DIRECTORY}/"));

    collect_files(target_directory, &target_patterns, true)
        .files
        .into_iter()
        .filter(|file| !received_paths.contains(file.relative.as_path()))
        .map(|file| TargetOnlyFile {
            relative: to_posix(&file.relative),
            size: file.size,
        })
        .collect()
}

pub fn apply(
    source: &Path,
    target_name: &str,
    target_directory: &Path,
    files: &[String],
) -> AppResult<ReceiveProjectResult> {
    let mut result = ReceiveProjectResult {
        target: target_name.to_string(),
        target_directory: target_directory.to_string_lossy().to_string(),
        added: 0,
        replaced: 0,
        bytes: 0,
        files: Vec::new(),
    };

    for relative in files {
        let clean = relative.trim_start_matches(['/', '\\']);
        if clean.is_empty() || clean.split(['/', '\\']).any(|segment| segment == "..") {
            continue;
        }

        let from = source.join(clean);
        if !from.is_file() {
            continue;
        }

        let to = target_directory.join(clean);
        let existed = to.exists();
        result.bytes += copy_file(&from, &to)?;
        if existed {
            result.replaced += 1;
        } else {
            result.added += 1;
        }
        result.files.push(clean.replace('\\', "/"));
    }

    Ok(result)
}

pub fn is_project_folder(path: &Path) -> bool {
    path.join(MANIFEST_FILE_NAME).is_file()
}
