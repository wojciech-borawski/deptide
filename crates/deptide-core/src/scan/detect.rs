use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::git::current_branch;
use crate::domain::{DetectedProject, PackageManifest};
use crate::util::json::read_json;
use crate::workspace::{relative_path, to_posix};

pub const MANIFEST_FILE_NAME: &str = "package.json";
pub const BUILD_SCRIPT_NAME: &str = "build";

pub const DEFAULT_IGNORED_DIRECTORIES: &[&str] = &[
    "node_modules",
    ".git",
    ".idea",
    ".vscode",
    "dist",
    "build",
    "out",
    "coverage",
    ".next",
    ".nuxt",
    ".output",
    ".turbo",
    ".cache",
    ".vite",
    ".vite-deps",
    "vendor",
    "storybook-static",
];

#[derive(Debug, Clone)]
pub struct DetectionOptions {
    pub max_depth: u32,
    pub ignored_directories: HashSet<String>,
}

pub fn build_ignored_directories(extra: &[String]) -> HashSet<String> {
    DEFAULT_IGNORED_DIRECTORIES
        .iter()
        .map(|name| name.to_string())
        .chain(extra.iter().map(|name| name.trim().to_string()))
        .filter(|name| !name.is_empty())
        .collect()
}

pub fn propose_name(relative_path: &str) -> String {
    let segments: Vec<&str> = relative_path.split('/').filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        return "root".to_string();
    }

    let mut kept: Vec<&str> = Vec::new();

    for segment in &segments {
        let lower = segment.to_lowercase();

        if let Some(previous) = kept.last().copied() {
            let previous_lower = previous.to_lowercase();

            if lower.contains(&previous_lower) {
                *kept.last_mut().expect("kept is not empty") = segment;
                continue;
            }

            if previous_lower.contains(&lower) {
                continue;
            }
        }

        kept.push(segment);
    }

    let tail_start = kept.len().saturating_sub(2);
    let name = kept[tail_start..].join("-");

    if name.is_empty() {
        segments[segments.len() - 1].to_string()
    } else {
        name
    }
}

pub fn read_package_manifest(directory: &Path) -> Option<PackageManifest> {
    read_json::<PackageManifest>(&directory.join(MANIFEST_FILE_NAME))
        .ok()
        .flatten()
}

fn to_detected_project(
    directory: &Path,
    root: &Path,
    manifest: PackageManifest,
) -> DetectedProject {
    let relative = relative_path(root, directory)
        .map(|path| to_posix(&path))
        .unwrap_or_else(|| to_posix(directory));

    let kind = manifest.kind();

    DetectedProject {
        directory: directory.to_string_lossy().to_string(),
        proposed_name: propose_name(&relative),
        relative_path: relative,
        package_name: manifest.name,
        version: manifest.version,
        branch: current_branch(directory),
        kind,
        has_build_script: manifest.scripts.contains_key(BUILD_SCRIPT_NAME),
        dependencies: manifest.dependencies,
        dev_dependencies: manifest.dev_dependencies,
    }
}

pub fn read_project_at(directory: &Path, root: &Path) -> Option<DetectedProject> {
    read_package_manifest(directory).map(|manifest| to_detected_project(directory, root, manifest))
}

pub fn detect_projects(root: &Path, options: &DetectionOptions) -> Vec<DetectedProject> {
    let mut found = Vec::new();
    let mut frontier: Vec<PathBuf> = vec![root.to_path_buf()];

    for _depth in 0..=options.max_depth {
        if frontier.is_empty() {
            break;
        }

        let mut next_frontier = Vec::new();

        for directory in &frontier {
            if let Some(manifest) = read_package_manifest(directory) {
                found.push(to_detected_project(directory, root, manifest));
            }

            let Ok(entries) = fs::read_dir(directory) else {
                continue;
            };

            for entry in entries.flatten() {
                let is_directory = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
                if !is_directory {
                    continue;
                }

                let name = entry.file_name().to_string_lossy().to_string();
                if options.ignored_directories.contains(&name) {
                    continue;
                }

                next_frontier.push(entry.path());
            }
        }

        frontier = next_frontier;
    }

    found.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    found
}
