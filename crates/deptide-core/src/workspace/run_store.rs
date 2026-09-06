use std::fs;
use std::path::PathBuf;

use chrono::Utc;
use serde::Serialize;

use super::Workspace;
use crate::domain::SavedRun;
use crate::error::{AppError, AppResult};
use crate::util::json::{read_json, write_json};
use crate::util::text::slugify;

const RUN_FILE_EXTENSION: &str = "json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedRunFile {
    pub file_name: String,
    pub file_path: String,
    pub run: SavedRun,
}

fn run_file_name(name: &str) -> String {
    let slug = slugify(name);
    let base = if slug.is_empty() { "run" } else { &slug };
    format!("{base}.{RUN_FILE_EXTENSION}")
}

pub fn suggest_run_name(package_names: &[String]) -> String {
    let date = Utc::now().format("%Y-%m-%d");
    let first = package_names
        .first()
        .and_then(|name| name.rsplit('/').next())
        .unwrap_or("update");
    let extra = if package_names.len() > 1 {
        format!("-plus-{}", package_names.len() - 1)
    } else {
        String::new()
    };

    format!("{date}-{first}{extra}")
}

pub fn list_runs(workspace: &Workspace) -> Vec<SavedRunFile> {
    let entries = match fs::read_dir(workspace.runs_directory()) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let mut runs: Vec<SavedRunFile> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext == RUN_FILE_EXTENSION)
        })
        .filter_map(|path| {
            let run = read_json::<SavedRun>(&path).ok().flatten()?;
            if run.packages.is_empty() {
                return None;
            }

            Some(SavedRunFile {
                file_name: path.file_name()?.to_string_lossy().to_string(),
                file_path: path.to_string_lossy().to_string(),
                run,
            })
        })
        .collect();

    runs.sort_by(|a, b| b.run.saved_at.cmp(&a.run.saved_at));
    runs
}

fn run_path(workspace: &Workspace, name: &str) -> PathBuf {
    workspace.runs_directory().join(run_file_name(name))
}

pub fn save_run(workspace: &Workspace, run: &SavedRun) -> AppResult<String> {
    let path = run_path(workspace, &run.name);
    write_json(&path, run)?;
    Ok(path.to_string_lossy().to_string())
}

pub fn load_run(workspace: &Workspace, name: &str) -> Option<SavedRun> {
    let wanted = run_file_name(name);

    list_runs(workspace)
        .into_iter()
        .find(|entry| entry.file_name == wanted || entry.run.name == name)
        .map(|entry| entry.run)
}

pub fn delete_run(workspace: &Workspace, name: &str) -> AppResult<()> {
    let wanted = run_file_name(name);
    let entry = list_runs(workspace)
        .into_iter()
        .find(|entry| entry.file_name == wanted || entry.run.name == name)
        .ok_or_else(|| AppError::new(format!("No saved run named {name}")))?;

    fs::remove_file(entry.file_path)?;
    Ok(())
}
