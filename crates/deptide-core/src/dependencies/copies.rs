use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::panic::resume_unwind;
use std::path::{Path, PathBuf};
use std::thread;

use super::projects::{WorkspaceProject, WorkspaceProjects};
use super::request::DependencyRequest;
use crate::domain::{
    CopyScope, InstalledCopy, PackageCopies, PackageManifest, ProjectCopies, UpdateConfig,
};
use crate::scan::MANIFEST_FILE_NAME;
use crate::util::json::read_json;
use crate::workspace::Workspace;

const NODE_MODULES: &str = "node_modules";
const MAX_NESTING: usize = 32;
const MAX_WORKERS: usize = 8;

struct CopyWalk<'a, R> {
    wanted: Option<&'a HashSet<String>>,
    request: &'a DependencyRequest,
    read_manifest: R,
    found: BTreeMap<String, Vec<(usize, InstalledCopy)>>,
    entered: HashSet<PathBuf>,
}

impl<R: FnMut(&Path) -> Option<PackageManifest>> CopyWalk<'_, R> {
    fn walk(&mut self, node_modules: &Path, relative: &str, nesting: usize) {
        if self.request.is_cancelled() {
            return;
        }
        let Ok(entries) = fs::read_dir(node_modules) else {
            return;
        };
        let identity = fs::canonicalize(node_modules).unwrap_or_else(|_| node_modules.into());
        if !self.entered.insert(identity) {
            return;
        }

        for entry in entries.flatten() {
            if self.request.is_cancelled() {
                return;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();

            if name.starts_with('@') {
                if !kind.is_dir() {
                    continue;
                }
                let Ok(scoped) = fs::read_dir(&path) else {
                    continue;
                };
                for inner in scoped.flatten() {
                    if self.request.is_cancelled() {
                        return;
                    }
                    let Ok(inner_kind) = inner.file_type() else {
                        continue;
                    };
                    let package = format!("{name}/{}", inner.file_name().to_string_lossy());
                    self.visit(
                        &inner.path(),
                        &package,
                        inner_kind.is_symlink(),
                        &format!("{relative}/{package}"),
                        nesting,
                    );
                }
            } else if kind.is_dir() || kind.is_symlink() {
                self.visit(
                    &path,
                    &name,
                    kind.is_symlink(),
                    &format!("{relative}/{name}"),
                    nesting,
                );
            }
        }
    }

    fn visit(
        &mut self,
        directory: &Path,
        package: &str,
        linked: bool,
        relative: &str,
        nesting: usize,
    ) {
        if self.wanted.map_or(true, |wanted| wanted.contains(package)) {
            let manifest = (self.read_manifest)(directory);
            if let Some(manifest) = manifest {
                self.found.entry(package.to_string()).or_default().push((
                    nesting,
                    InstalledCopy {
                        version: manifest.version,
                        path: relative.to_string(),
                    },
                ));
            }
        }

        if !linked && nesting < MAX_NESTING {
            self.walk(
                &directory.join(NODE_MODULES),
                &format!("{relative}/{NODE_MODULES}"),
                nesting + 1,
            );
        }
    }

    fn into_duplicates(self) -> Vec<PackageCopies> {
        self.found
            .into_iter()
            .filter(|(_, copies)| copies.len() > 1)
            .map(|(name, mut copies)| {
                copies.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.path.cmp(&b.1.path)));
                PackageCopies {
                    name,
                    copies: copies.into_iter().map(|(_, copy)| copy).collect(),
                }
            })
            .collect()
    }
}

fn read_manifest_file(package: &Path) -> Option<PackageManifest> {
    read_json(&package.join(MANIFEST_FILE_NAME)).ok().flatten()
}

fn scan_project(
    project: &WorkspaceProject,
    wanted: Option<&HashSet<String>>,
    request: &DependencyRequest,
) -> ProjectCopies {
    let node_modules = project.directory.join(NODE_MODULES);
    let mut walk = CopyWalk {
        wanted,
        request,
        read_manifest: read_manifest_file,
        found: BTreeMap::new(),
        entered: HashSet::new(),
    };
    walk.walk(&node_modules, NODE_MODULES, 0);
    ProjectCopies {
        project: project.name().to_string(),
        has_node_modules: node_modules.is_dir(),
        packages: walk.into_duplicates(),
    }
}

pub fn find_installed_copies(
    workspace: &Workspace,
    config: &UpdateConfig,
    scope: CopyScope,
    request: &DependencyRequest,
) -> Vec<ProjectCopies> {
    let read = WorkspaceProjects::read(workspace, config);
    let libraries: HashSet<String> = read.library_names(config).into_iter().collect();
    let wanted = match scope {
        CopyScope::Libraries => Some(&libraries),
        CopyScope::AllPackages => None,
    };

    let workers = thread::available_parallelism()
        .map_or(1, usize::from)
        .clamp(1, MAX_WORKERS);
    let chunk = read.projects.len().div_ceil(workers).max(1);

    thread::scope(|scope| {
        let handles: Vec<_> = read
            .projects
            .chunks(chunk)
            .map(|projects| {
                scope.spawn(move || {
                    projects
                        .iter()
                        .map(|project| scan_project(project, wanted, request))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().unwrap_or_else(|panic| resume_unwind(panic)))
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest_reads_until_the_first_read_cancels(name: &str, packages: &[String]) -> usize {
        let root = std::env::temp_dir().join(format!(
            "deptide-copies-cancel-{name}-{}",
            std::process::id()
        ));
        let node_modules = root.join(NODE_MODULES);
        for package in packages {
            let folder = node_modules.join(package);
            fs::create_dir_all(&folder).unwrap();
            let manifest = format!(r#"{{"name":"{package}","version":"1.0.0"}}"#);
            fs::write(folder.join(MANIFEST_FILE_NAME), manifest).unwrap();
        }
        let request = DependencyRequest::new();
        let mut reads = 0;
        let mut walk = CopyWalk {
            wanted: None,
            request: &request,
            read_manifest: |package: &Path| {
                reads += 1;
                request.cancel();
                read_manifest_file(package)
            },
            found: BTreeMap::new(),
            entered: HashSet::new(),
        };

        walk.walk(&node_modules, NODE_MODULES, 0);
        drop(walk);
        fs::remove_dir_all(&root).unwrap();
        reads
    }

    #[test]
    fn cancelling_during_the_walk_stops_further_manifest_reads() {
        let packages: Vec<String> = (0..16)
            .flat_map(|index| [format!("@scope/scoped-{index}"), format!("flat-{index}")])
            .collect();

        assert_eq!(
            manifest_reads_until_the_first_read_cancels("mixed", &packages),
            1
        );
    }

    #[test]
    fn cancelling_during_a_walk_of_unscoped_packages_stops_further_manifest_reads() {
        let packages: Vec<String> = (0..16).map(|index| format!("flat-{index}")).collect();

        assert_eq!(
            manifest_reads_until_the_first_read_cancels("flat", &packages),
            1
        );
    }
}
