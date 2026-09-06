mod clipboard;
mod files;
mod journal;
mod receive;
mod staging;

use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use journal::record_transfer;
use staging::create_staging_directory;

pub use files::{collect_files, copy_collection, same_content, Collection};
pub use receive::{
    analyze, apply, is_project_folder, suggest_target, FileStatus, KnownProject, ReceiveFile,
    ReceiveProjectPlan, ReceiveProjectResult, ReceiveSelection,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardContents {
    pub entries: Vec<ClipboardEntry>,
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

fn combined_ignore_patterns(config: &UpdateConfig, extra: &[String]) -> Vec<String> {
    config
        .transfer_ignore
        .iter()
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
) -> AppResult<TransferPreview> {
    let patterns = combined_ignore_patterns(config, extra_patterns);
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
) -> AppResult<CopyResult> {
    let patterns = combined_ignore_patterns(config, extra_patterns);
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

fn known_projects(workspace: &Workspace, config: &UpdateConfig) -> Vec<KnownProject> {
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
    let folders = clipboard::directories(&clipboard::file_list());
    let known = known_projects(workspace, config);

    let entries = folders
        .into_iter()
        .map(|folder| ClipboardEntry {
            name: folder
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default(),
            is_project: is_project_folder(&folder),
            package_name: read_project_at(&folder, &folder).and_then(|found| found.package_name),
            suggested_project: suggest_target(&folder, &known),
            path: folder.to_string_lossy().to_string(),
        })
        .collect();

    ClipboardContents { entries }
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
) -> AppResult<ReceivePlan> {
    let patterns = combined_ignore_patterns(config, extra_patterns);
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

pub fn apply_receive(
    workspace: &Workspace,
    config: &UpdateConfig,
    selections: &[ReceiveSelection],
) -> AppResult<ReceiveResult> {
    let mut projects = Vec::new();

    for selection in selections {
        let target = target_directory(workspace, config, &selection.target)?;
        projects.push(apply(
            Path::new(&selection.source),
            &selection.target,
            &target,
            &selection.files,
        )?);
    }

    let mut result = ReceiveResult {
        files: projects.iter().map(|project| project.files.len()).sum(),
        bytes: projects.iter().map(|project| project.bytes).sum(),
        projects,
        received_at: iso_timestamp(Utc::now()),
        log_file: None,
    };
    result.log_file = record_transfer(workspace, "receive", &result);

    Ok(result)
}
