use deptide_core::dependencies::{
    apply_latest_versions, fetch_latest_versions, find_installed_copies, read_dependency_drift,
    read_dependency_graph, DependencyRequest, LatestLookup,
};
use deptide_core::domain::{
    CopyScope, DependencyDrift, DependencyGraph, DependencySection, DriftCell, LibraryDrift,
    ProjectCopies,
};
use deptide_core::error::AppResult;
use deptide_core::workspace::open_with_config;

use crate::arguments::DepsArgs;
use crate::exit_code::ExitCode;

const DECLARED_WIDTH: usize = 17;
const INSTALLED_WIDTH: usize = 15;

fn library_heading(library: &LibraryDrift) -> String {
    let ambiguous = (!library.ambiguous_projects.is_empty()).then(|| {
        format!(
            "local version ambiguous: {}",
            library.ambiguous_projects.join(", ")
        )
    });
    let local = library
        .local_version
        .as_ref()
        .map(|version| match &library.local_project {
            Some(project) => format!("local {version} ({project})"),
            None => format!("local {version}"),
        })
        .or(ambiguous);
    let latest = match (&library.latest, &library.latest_error) {
        (Some(version), _) => Some(format!("latest {version}")),
        (None, Some(error)) => Some(format!("latest unknown: {error}")),
        (None, None) => None,
    };
    let details: Vec<String> = local.into_iter().chain(latest).collect();
    if details.is_empty() {
        library.name.clone()
    } else {
        format!("{}  {}", library.name, details.join(", "))
    }
}

fn cell_line(cell: &DriftCell, project_width: usize) -> String {
    let declared = match cell.section {
        DependencySection::Dependencies => cell.declared.clone(),
        DependencySection::Dev => format!("{} (dev)", cell.declared),
        DependencySection::Peer => format!("{} (peer)", cell.declared),
    };
    let installed = cell.installed.as_deref().unwrap_or("not installed");
    let markers: Vec<&str> = [
        (cell.out_of_range, "out of range"),
        (cell.differs, "differs"),
        (cell.behind, "behind"),
    ]
    .into_iter()
    .filter(|(flag, _)| *flag)
    .map(|(_, marker)| marker)
    .collect();
    let line = format!(
        "  {:<project_width$}{declared:<DECLARED_WIDTH$}{installed:<INSTALLED_WIDTH$}{}",
        cell.project,
        markers.join(", ")
    );
    line.trim_end().to_string()
}

fn copies_lines(copies: &[ProjectCopies]) -> Vec<String> {
    let mut lines = vec!["Duplicate copies".to_string()];

    for project in copies {
        if !project.has_node_modules {
            lines.push(format!("{}  no node_modules folder", project.project));
            continue;
        }
        if project.packages.is_empty() {
            lines.push(format!("{}  no duplicate copies", project.project));
            continue;
        }
        lines.push(project.project.clone());
        for package in &project.packages {
            lines.push(format!("  {}", package.name));
            lines.extend(package.copies.iter().map(|copy| {
                format!(
                    "    {}  {}",
                    copy.version.as_deref().unwrap_or("?"),
                    copy.path
                )
            }));
        }
    }

    lines
}

fn order_lines(graph: &DependencyGraph) -> Vec<String> {
    let steps = graph.nodes.iter().enumerate().map(|(index, node)| {
        if node.depends_on.is_empty() {
            format!("  {}. {}", index + 1, node.name)
        } else {
            format!(
                "  {}. {}  waits for {}",
                index + 1,
                node.name,
                node.depends_on.join(", ")
            )
        }
    });
    let cycles = graph.cycles.iter().map(|cycle| {
        format!(
            "Cycle, the run builds these last without waiting between them: {}",
            cycle.join(", ")
        )
    });

    std::iter::once("Build order".to_string())
        .chain(steps)
        .chain(cycles)
        .collect()
}

fn drift_lines(drift: &DependencyDrift) -> Vec<String> {
    let project_width = drift.projects.iter().map(String::len).max().unwrap_or(0) + 2;
    let mut lines = Vec::new();

    for library in &drift.libraries {
        lines.push(library_heading(library));
        lines.extend(
            library
                .cells
                .iter()
                .map(|cell| cell_line(cell, project_width)),
        );
    }
    if !drift.unreadable.is_empty() {
        lines.push(format!("not readable: {}", drift.unreadable.join(", ")));
    }

    lines
}

pub async fn execute(args: DepsArgs) -> AppResult<ExitCode> {
    let (workspace, config) = open_with_config(&args.workspace)?;

    let request = DependencyRequest::new();
    let mut drift = read_dependency_drift(&workspace, &config);
    if args.latest {
        let lookup = LatestLookup::new(request.clone());
        let latest = fetch_latest_versions(&workspace, &config, &drift, &lookup).await;
        apply_latest_versions(&mut drift, &latest);
    }
    let mut lines = drift_lines(&drift);

    lines.push(String::new());
    lines.extend(order_lines(&read_dependency_graph(&workspace, &config)));

    if args.duplicates {
        let scope = if args.all_packages {
            CopyScope::AllPackages
        } else {
            CopyScope::Libraries
        };
        lines.push(String::new());
        lines.extend(copies_lines(&find_installed_copies(
            &workspace, &config, scope, &request,
        )));
    }

    for line in lines {
        println!("{line}");
    }
    Ok(ExitCode::Success)
}

#[cfg(test)]
mod tests {
    use deptide_core::domain::{
        DependencyDrift, DependencyGraph, DependencySection, DriftCell, GraphNode, InstalledCopy,
        LibraryDrift, PackageCopies, ProjectCopies,
    };

    use super::{copies_lines, drift_lines, order_lines};

    fn cell(project: &str, declared: &str, installed: Option<&str>) -> DriftCell {
        DriftCell {
            project: project.to_string(),
            section: DependencySection::Dependencies,
            declared: declared.to_string(),
            installed: installed.map(str::to_string),
            out_of_range: false,
            differs: false,
            behind: false,
        }
    }

    #[test]
    fn the_drift_table_lists_each_library_with_one_line_per_project_and_its_markers() {
        let drift = DependencyDrift {
            projects: vec!["core".into(), "web".into(), "admin-portal".into()],
            unreadable: vec!["gone".into()],
            libraries: vec![
                LibraryDrift {
                    name: "@acme/core".into(),
                    local_project: Some("core".into()),
                    local_version: Some("3.2.0".into()),
                    ambiguous_projects: vec![],
                    newest_installed: Some("3.1.4".into()),
                    latest: Some("3.3.0".into()),
                    latest_error: None,
                    mismatch: true,
                    cells: vec![
                        DriftCell {
                            differs: true,
                            behind: true,
                            ..cell("web", "^3.1.0", Some("3.1.4"))
                        },
                        DriftCell {
                            section: DependencySection::Dev,
                            out_of_range: true,
                            differs: true,
                            behind: true,
                            ..cell("admin-portal", "~3.0.0", Some("3.1.0"))
                        },
                    ],
                },
                LibraryDrift {
                    name: "left-pad".into(),
                    local_project: None,
                    local_version: None,
                    ambiguous_projects: vec![],
                    newest_installed: None,
                    latest: None,
                    latest_error: Some("npm error code E404".into()),
                    mismatch: false,
                    cells: vec![cell("web", "^1.0.0", None)],
                },
            ],
        };

        assert_eq!(
            drift_lines(&drift),
            vec![
                "@acme/core  local 3.2.0 (core), latest 3.3.0",
                "  web           ^3.1.0           3.1.4          differs, behind",
                "  admin-portal  ~3.0.0 (dev)     3.1.0          out of range, differs, behind",
                "left-pad  latest unknown: npm error code E404",
                "  web           ^1.0.0           not installed",
                "not readable: gone",
            ]
        );
    }

    fn node(name: &str, depends_on: &[&str]) -> GraphNode {
        GraphNode {
            name: name.to_string(),
            package_name: None,
            layer: 0,
            depends_on: depends_on.iter().map(|name| name.to_string()).collect(),
        }
    }

    #[test]
    fn a_library_two_projects_provide_is_shown_as_ambiguous() {
        let drift = DependencyDrift {
            projects: vec!["core".into(), "core-copy".into(), "web".into()],
            unreadable: vec![],
            libraries: vec![LibraryDrift {
                name: "@acme/core".into(),
                local_project: None,
                local_version: None,
                ambiguous_projects: vec!["core".into(), "core-copy".into()],
                newest_installed: None,
                latest: None,
                latest_error: None,
                mismatch: false,
                cells: vec![cell("web", "^3.1.0", None)],
            }],
        };

        assert_eq!(
            drift_lines(&drift)[0],
            "@acme/core  local version ambiguous: core, core-copy"
        );
    }

    #[test]
    fn the_build_order_is_numbered_with_what_each_project_waits_for_and_names_cycles() {
        let graph = DependencyGraph {
            nodes: vec![
                node("core", &[]),
                node("ui", &["core"]),
                node("web", &["core", "ui"]),
                node("x", &[]),
                node("y", &[]),
            ],
            edges: Vec::new(),
            cycles: vec![vec!["x".into(), "y".into()]],
        };

        assert_eq!(
            order_lines(&graph),
            vec![
                "Build order",
                "  1. core",
                "  2. ui  waits for core",
                "  3. web  waits for core, ui",
                "  4. x",
                "  5. y",
                "Cycle, the run builds these last without waiting between them: x, y",
            ]
        );
    }

    fn copy(version: Option<&str>, path: &str) -> InstalledCopy {
        InstalledCopy {
            version: version.map(str::to_string),
            path: path.to_string(),
        }
    }

    #[test]
    fn duplicate_copies_are_listed_per_project_with_version_and_path() {
        let copies = vec![
            ProjectCopies {
                project: "web".into(),
                has_node_modules: true,
                packages: vec![PackageCopies {
                    name: "@acme/core".into(),
                    copies: vec![
                        copy(Some("3.1.0"), "node_modules/@acme/core"),
                        copy(None, "node_modules/@acme/ui/node_modules/@acme/core"),
                    ],
                }],
            },
            ProjectCopies {
                project: "api".into(),
                has_node_modules: true,
                packages: Vec::new(),
            },
            ProjectCopies {
                project: "admin".into(),
                has_node_modules: false,
                packages: Vec::new(),
            },
        ];

        assert_eq!(
            copies_lines(&copies),
            vec![
                "Duplicate copies",
                "web",
                "  @acme/core",
                "    3.1.0  node_modules/@acme/core",
                "    ?  node_modules/@acme/ui/node_modules/@acme/core",
                "api  no duplicate copies",
                "admin  no node_modules folder",
            ]
        );
    }
}
