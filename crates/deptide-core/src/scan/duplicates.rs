use serde::Serialize;

use crate::domain::ConfiguredProject;
use crate::workspace::{normalize_directory, Workspace};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroup {
    pub directory: String,
    pub display_path: String,
    pub entries: Vec<String>,
}

pub fn find_duplicate_groups(
    projects: &[ConfiguredProject],
    workspace: &Workspace,
) -> Vec<DuplicateGroup> {
    let mut groups: Vec<DuplicateGroup> = Vec::new();

    for project in projects {
        let directory = normalize_directory(&workspace.resolve_project(&project.path));

        match groups.iter_mut().find(|group| group.directory == directory) {
            Some(group) => group.entries.push(project.name.clone()),
            None => groups.push(DuplicateGroup {
                directory,
                display_path: project.path.clone(),
                entries: vec![project.name.clone()],
            }),
        }
    }

    groups.retain(|group| group.entries.len() > 1);
    groups
}
