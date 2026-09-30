mod clipboard;
mod files;
mod journal;
mod merge;
mod preview;
mod receive;
mod receive_clipboard;
mod recycle;
mod staging;
mod virtual_files;

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use journal::record_transfer;
use staging::create_staging_directory;

pub use clipboard::{clipboard_sequence, read_clipboard_files, ClipboardFiles};
pub use files::{collect_files, copy_collection, same_content, Collection};
pub use merge::{merge_lines, ChunkError, LineChunk};
pub use preview::{read_file_pair, FileSide, ReceiveFileContents, PREVIEW_LIMIT};
pub use receive::{
    analyze, apply, is_project_folder, suggest_target, suggest_target_for, FileMerge, FileStatus,
    KnownProject, MergedFile, ReceiveFile, ReceiveProjectPlan, ReceiveProjectResult,
    ReceiveSelection,
};
pub use receive_clipboard::{
    ClipboardAccess, ClipboardDownload, DownloadedFolder, ReceiveClipboard, SystemClipboard,
};
pub use recycle::{Recycler, SystemRecycler};
pub use staging::create_receive_directory;
pub use virtual_files::{
    extract_virtual, read_virtual_file, sanitize_relative_path, top_level_folders, ExtractProgress,
    VirtualEntry, VirtualFolder, VirtualListing,
};

use crate::domain::UpdateConfig;
use crate::error::{AppError, AppResult};
use crate::scan::read_project_at;
use crate::util::text::slugify;
use crate::util::time::iso_timestamp;
use crate::workspace::Workspace;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferProjectSummary {
    pub name: String,
    pub directory: String,
    pub files: usize,
    pub bytes: u64,
    pub skipped: usize,
    pub staged_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferPreview {
    pub projects: Vec<TransferProjectSummary>,
    pub files: usize,
    pub bytes: u64,
    pub patterns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyResult {
    pub staging_directory: String,
    pub projects: Vec<TransferProjectSummary>,
    pub files: usize,
    pub bytes: u64,
    pub copied_at: String,
    pub log_file: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardEntry {
    pub path: String,
    pub name: String,
    pub is_project: bool,
    pub package_name: Option<String>,
    pub suggested_project: Option<String>,
    pub files: Option<usize>,
    pub bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClipboardSource {
    Paths,
    Virtual,
    Empty,
    Busy,
    Unreadable,
}

/// `sequence` is set for virtual files, whose entry paths are ids that `ReceiveClipboard::download` resolves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardContents {
    pub source: ClipboardSource,
    pub sequence: Option<u32>,
    pub entries: Vec<ClipboardEntry>,
    pub rejected: usize,
    pub problem: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiveRequest {
    pub source: String,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceivePlan {
    pub projects: Vec<ReceiveProjectPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiveResult {
    pub projects: Vec<ReceiveProjectResult>,
    pub files: usize,
    pub bytes: u64,
    pub received_at: String,
    pub log_file: Option<String>,
}

fn combined_ignore_patterns(
    config: &UpdateConfig,
    extra: &[String],
    disabled: &[String],
) -> Vec<String> {
    let disabled: HashSet<&str> = disabled.iter().map(|pattern| pattern.trim()).collect();

    config
        .transfer_ignore
        .iter()
        .filter(|pattern| !disabled.contains(pattern.trim()))
        .chain(extra.iter())
        .map(|pattern| pattern.trim().to_string())
        .filter(|pattern| !pattern.is_empty())
        .collect()
}

fn selected_projects<'a>(
    workspace: &Workspace,
    config: &'a UpdateConfig,
    names: &[String],
) -> AppResult<Vec<(&'a str, PathBuf)>> {
    let mut projects = Vec::new();

    for name in names {
        let project = config
            .projects
            .iter()
            .find(|project| &project.name == name)
            .ok_or_else(|| AppError::new(format!("Unknown project {name}")))?;
        let directory = workspace.resolve_project(&project.path);
        if !directory.is_dir() {
            return Err(AppError::new(format!(
                "Folder of {name} does not exist: {}",
                directory.display()
            )));
        }
        projects.push((project.name.as_str(), directory));
    }

    Ok(projects)
}

fn summarize_project_transfer(
    name: &str,
    directory: &Path,
    collection: &Collection,
    staged: Option<&Path>,
) -> TransferProjectSummary {
    TransferProjectSummary {
        name: name.to_string(),
        directory: directory.to_string_lossy().to_string(),
        files: collection.files.len(),
        bytes: collection.bytes(),
        skipped: collection.skipped,
        staged_path: staged.map(|path| path.to_string_lossy().to_string()),
    }
}

pub fn preview(
    workspace: &Workspace,
    config: &UpdateConfig,
    names: &[String],
    extra_patterns: &[String],
    disabled_patterns: &[String],
) -> AppResult<TransferPreview> {
    let patterns = combined_ignore_patterns(config, extra_patterns, disabled_patterns);
    let projects: Vec<TransferProjectSummary> = selected_projects(workspace, config, names)?
        .into_iter()
        .map(|(name, directory)| {
            let collection = collect_files(&directory, &patterns, true);
            summarize_project_transfer(name, &directory, &collection, None)
        })
        .collect();

    Ok(TransferPreview {
        files: projects.iter().map(|project| project.files).sum(),
        bytes: projects.iter().map(|project| project.bytes).sum(),
        projects,
        patterns,
    })
}

pub fn copy_to_clipboard(
    workspace: &Workspace,
    config: &UpdateConfig,
    names: &[String],
    extra_patterns: &[String],
    disabled_patterns: &[String],
) -> AppResult<CopyResult> {
    let patterns = combined_ignore_patterns(config, extra_patterns, disabled_patterns);
    let projects = selected_projects(workspace, config, names)?;
    let staging = create_staging_directory()?;

    let mut summaries = Vec::new();
    let mut staged_paths = Vec::new();

    for (name, directory) in projects {
        let collection = collect_files(&directory, &patterns, true);
        let target = staging.join(slugify(name).replace('.', "_"));
        fs::create_dir_all(&target)?;
        copy_collection(&collection, &target)?;
        summaries.push(summarize_project_transfer(
            name,
            &directory,
            &collection,
            Some(&target),
        ));
        staged_paths.push(target);
    }

    clipboard::set_file_list(&staged_paths)?;

    let mut result = CopyResult {
        staging_directory: staging.to_string_lossy().to_string(),
        files: summaries.iter().map(|project| project.files).sum(),
        bytes: summaries.iter().map(|project| project.bytes).sum(),
        projects: summaries,
        copied_at: iso_timestamp(Utc::now()),
        log_file: None,
    };
    result.log_file = record_transfer(workspace, "copy", &result);

    Ok(result)
}

pub fn known_projects(workspace: &Workspace, config: &UpdateConfig) -> Vec<KnownProject> {
    config
        .projects
        .iter()
        .map(|project| {
            let directory = workspace.resolve_project(&project.path);
            KnownProject {
                name: project.name.clone(),
                package_name: read_project_at(&directory, &directory)
                    .and_then(|found| found.package_name),
                directory,
            }
        })
        .collect()
}

pub fn inspect_clipboard(workspace: &Workspace, config: &UpdateConfig) -> ClipboardContents {
    ReceiveClipboard::default().inspect(&SystemClipboard, &known_projects(workspace, config))
}

fn target_directory(
    workspace: &Workspace,
    config: &UpdateConfig,
    name: &str,
) -> AppResult<PathBuf> {
    config
        .projects
        .iter()
        .find(|project| project.name == name)
        .map(|project| workspace.resolve_project(&project.path))
        .ok_or_else(|| AppError::new(format!("Unknown project {name}")))
}

pub fn analyze_receive(
    workspace: &Workspace,
    config: &UpdateConfig,
    requests: &[ReceiveRequest],
    extra_patterns: &[String],
    disabled_patterns: &[String],
) -> AppResult<ReceivePlan> {
    let patterns = combined_ignore_patterns(config, extra_patterns, disabled_patterns);
    let mut projects = Vec::new();

    for request in requests {
        let source = Path::new(&request.source);
        if !source.is_dir() {
            return Err(AppError::new(format!(
                "Source folder is gone: {}",
                request.source
            )));
        }
        let target = target_directory(workspace, config, &request.target)?;
        projects.push(analyze(source, &request.target, &target, &patterns));
    }

    Ok(ReceivePlan { projects })
}

pub fn read_receive_file(
    workspace: &Workspace,
    config: &UpdateConfig,
    source: &str,
    target: &str,
    relative: &str,
) -> AppResult<ReceiveFileContents> {
    let source_directory = Path::new(source);
    if !source_directory.is_dir() {
        return Err(AppError::new(format!("Source folder is gone: {source}")));
    }
    let target_directory = target_directory(workspace, config, target)?;
    read_file_pair(source_directory, &target_directory, relative)
}

pub fn apply_receive(
    workspace: &Workspace,
    config: &UpdateConfig,
    selections: &[ReceiveSelection],
) -> AppResult<ReceiveResult> {
    apply_receive_with(workspace, config, selections, &SystemRecycler)
}

pub fn apply_receive_with(
    workspace: &Workspace,
    config: &UpdateConfig,
    selections: &[ReceiveSelection],
    recycler: &dyn Recycler,
) -> AppResult<ReceiveResult> {
    let targets = selections
        .iter()
        .map(|selection| target_directory(workspace, config, &selection.target))
        .collect::<AppResult<Vec<_>>>()?;
    let mut projects = Vec::new();

    for (selection, target) in selections.iter().zip(&targets) {
        match apply(selection, target, recycler) {
            Ok(project) => projects.push(project),
            Err(error) if projects.is_empty() => return Err(error),
            Err(error) => {
                let received: Vec<String> = projects
                    .iter()
                    .map(|project| project.target.clone())
                    .collect();
                record_receive(workspace, projects);
                return Err(AppError::new(format!(
                    "{error}. Already received: {}",
                    received.join(", ")
                )));
            }
        }
    }

    Ok(record_receive(workspace, projects))
}

fn record_receive(workspace: &Workspace, projects: Vec<ReceiveProjectResult>) -> ReceiveResult {
    let mut result = ReceiveResult {
        files: projects.iter().map(|project| project.files.len()).sum(),
        bytes: projects.iter().map(|project| project.bytes).sum(),
        projects,
        received_at: iso_timestamp(Utc::now()),
        log_file: None,
    };
    result.log_file = record_transfer(workspace, "receive", &result);
    result
}
