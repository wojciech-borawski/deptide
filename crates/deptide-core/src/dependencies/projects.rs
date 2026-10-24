use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::domain::{DependencySection, DetectedProject, PackageManifest, UpdateConfig};
use crate::scan::{collect_dependency_candidates, read_project_at, MANIFEST_FILE_NAME};
use crate::util::json::read_json;
use crate::workspace::Workspace;

pub(crate) struct WorkspaceProject {
    pub directory: PathBuf,
    pub detected: DetectedProject,
}

impl WorkspaceProject {
    pub fn name(&self) -> &str {
        &self.detected.proposed_name
    }

    pub fn declared(&self, package: &str) -> Option<(DependencySection, &str)> {
        self.detected.declared().find(package)
    }
}

pub(crate) struct WorkspaceProjects {
    pub projects: Vec<WorkspaceProject>,
    pub unreadable: Vec<String>,
}

impl WorkspaceProjects {
    pub fn read(workspace: &Workspace, config: &UpdateConfig) -> Self {
        let mut projects = Vec::new();
        let mut unreadable = Vec::new();

        for configured in &config.projects {
            let directory = workspace.resolve_project(&configured.path);
            match read_project_at(&directory, workspace.root()) {
                Some(mut detected) => {
                    detected.proposed_name = configured.name.clone();
                    projects.push(WorkspaceProject {
                        directory,
                        detected,
                    });
                }
                None => unreadable.push(configured.name.clone()),
            }
        }

        Self {
            projects,
            unreadable,
        }
    }

    pub fn names(&self) -> Vec<String> {
        self.projects
            .iter()
            .map(|project| project.name().to_string())
            .collect()
    }

    pub fn providers(&self) -> Providers<'_, &WorkspaceProject> {
        group_providers(
            self.projects
                .iter()
                .filter_map(|project| Some((project.detected.package_name.as_deref()?, project))),
        )
    }

    pub fn library_names(&self, config: &UpdateConfig) -> Vec<String> {
        let detected: Vec<DetectedProject> = self
            .projects
            .iter()
            .map(|project| project.detected.clone())
            .collect();
        let providers = self.providers();

        collect_dependency_candidates(&detected, &detected, "")
            .into_iter()
            .filter(|candidate| {
                providers.single.contains_key(candidate.name.as_str())
                    || providers.ambiguous.contains_key(candidate.name.as_str())
                    || config
                        .packages
                        .iter()
                        .any(|package| package.name == candidate.name)
            })
            .map(|candidate| candidate.name)
            .collect()
    }
}

pub(crate) struct Providers<'a, T> {
    pub single: BTreeMap<&'a str, T>,
    pub ambiguous: BTreeMap<&'a str, Vec<T>>,
}

pub(crate) fn group_providers<'a, T>(
    entries: impl IntoIterator<Item = (&'a str, T)>,
) -> Providers<'a, T> {
    let mut grouped: BTreeMap<&str, Vec<T>> = BTreeMap::new();
    for (package, provider) in entries {
        grouped.entry(package).or_default().push(provider);
    }

    let mut providers = Providers {
        single: BTreeMap::new(),
        ambiguous: BTreeMap::new(),
    };
    for (package, mut found) in grouped {
        if found.len() == 1 {
            providers.single.insert(package, found.remove(0));
        } else {
            providers.ambiguous.insert(package, found);
        }
    }
    providers
}

pub(super) fn installed_version(project_directory: &Path, package: &str) -> Option<String> {
    let mut path = project_directory.join("node_modules");
    for segment in package.split('/') {
        path.push(segment);
    }
    read_json::<PackageManifest>(&path.join(MANIFEST_FILE_NAME))
        .ok()
        .flatten()
        .and_then(|manifest| manifest.version)
}
