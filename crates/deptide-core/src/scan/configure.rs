use std::collections::HashSet;
use std::path::Path;

use serde::Serialize;

use crate::domain::{ConfiguredProject, DetectedProject, UpdateConfig};
use crate::util::text::unique_name;
use crate::workspace::{normalize_directory, Workspace};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSelectionOutcome {
    pub config: UpdateConfig,
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

pub fn apply_scan_selection(
    workspace: &Workspace,
    config: &UpdateConfig,
    offered: &[DetectedProject],
    selected_directories: &[String],
) -> ScanSelectionOutcome {
    let offered_directories: HashSet<String> = offered
        .iter()
        .map(|project| normalize_directory(Path::new(&project.directory)))
        .collect();

    let selected: HashSet<String> = selected_directories
        .iter()
        .map(|directory| normalize_directory(Path::new(directory)))
        .collect();

    let configured_directory = |project: &ConfiguredProject| {
        normalize_directory(&workspace.resolve_project(&project.path))
    };

    let mut removed = Vec::new();
    let mut projects: Vec<ConfiguredProject> = Vec::new();

    for project in &config.projects {
        let directory = configured_directory(project);
        let dropped = offered_directories.contains(&directory) && !selected.contains(&directory);

        if dropped {
            removed.push(project.name.clone());
        } else {
            projects.push(project.clone());
        }
    }

    let configured: HashSet<String> = projects.iter().map(configured_directory).collect();
    let mut taken: Vec<String> = projects
        .iter()
        .map(|project| project.name.clone())
        .collect();
    let mut added = Vec::new();

    for project in offered {
        let directory = normalize_directory(Path::new(&project.directory));
        if !selected.contains(&directory) || configured.contains(&directory) {
            continue;
        }

        let name = unique_name(&project.proposed_name, &taken);
        taken.push(name.clone());
        added.push(name.clone());

        projects.push(ConfiguredProject::new(
            name,
            workspace.to_config_path(Path::new(&project.directory)),
        ));
    }

    ScanSelectionOutcome {
        config: UpdateConfig {
            projects,
            ..config.clone()
        },
        added,
        removed,
    }
}
