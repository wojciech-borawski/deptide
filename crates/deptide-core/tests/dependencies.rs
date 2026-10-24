mod common;

use common::{link_folder, manifest, TempDir};
use deptide_core::dependencies::{
    apply_latest_versions, find_installed_copies, read_dependency_drift, read_dependency_graph,
    DependencyRequest,
};
use deptide_core::domain::{
    ConfiguredProject, CopyScope, DependencySection, ExecutionMode, LatestVersion, PackageSpec,
    ProjectCopies, RunPlan, StepName, UpdateConfig, VersionPolicy,
};
use deptide_core::execution::build_jobs;
use deptide_core::workspace::Workspace;

fn config_of(projects: &[(&str, &str)]) -> UpdateConfig {
    UpdateConfig {
        projects: projects
            .iter()
            .map(|(name, path)| ConfiguredProject::new(*name, *path))
            .collect(),
        ..UpdateConfig::default()
    }
}

#[test]
fn drift_reads_installed_versions_from_node_modules_including_scoped_packages() {
    let root = TempDir::new("drift-installed");
    root.write(
        "libs/core/package.json",
        &manifest("@acme/core", "3.2.0", &[]),
    );
    root.write(
        "apps/web/package.json",
        &manifest(
            "web",
            "1.0.0",
            &[("@acme/core", "^3.1.0"), ("lodash", "^4.0.0")],
        ),
    );
    root.write(
        "apps/web/node_modules/@acme/core/package.json",
        &manifest("@acme/core", "3.1.4", &[]),
    );
    root.write(
        "apps/admin/package.json",
        &manifest("admin", "1.0.0", &[("@acme/core", "^3.0.0")]),
    );
    let workspace = Workspace::open(root.path()).unwrap();
    let config = config_of(&[
        ("core", "libs/core"),
        ("web", "apps/web"),
        ("admin", "apps/admin"),
    ]);

    let drift = read_dependency_drift(&workspace, &config);

    assert_eq!(drift.projects, vec!["core", "web", "admin"]);
    let names: Vec<&str> = drift.libraries.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, vec!["@acme/core"]);
    let core = &drift.libraries[0];
    assert_eq!(core.local_project.as_deref(), Some("core"));
    assert_eq!(core.local_version.as_deref(), Some("3.2.0"));
    let cells: Vec<(&str, &str, Option<&str>)> = core
        .cells
        .iter()
        .map(|cell| {
            (
                cell.project.as_str(),
                cell.declared.as_str(),
                cell.installed.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        cells,
        vec![("web", "^3.1.0", Some("3.1.4")), ("admin", "^3.0.0", None)]
    );
    assert!(core
        .cells
        .iter()
        .all(|cell| cell.section == DependencySection::Dependencies));
}

#[test]
fn drift_marks_versions_outside_the_declared_range_and_versions_that_differ_across_projects() {
    let root = TempDir::new("drift-mismatch");
    root.write(
        "libs/core/package.json",
        &manifest("@acme/core", "3.2.0", &[]),
    );
    root.write("libs/ui/package.json", &manifest("@acme/ui", "1.0.0", &[]));
    root.write(
        "apps/web/package.json",
        &manifest(
            "web",
            "1.0.0",
            &[
                ("@acme/core", "^3.1.0-ABC-1"),
                ("@acme/ui", "file:../../libs/ui"),
            ],
        ),
    );
    root.write(
        "apps/web/node_modules/@acme/core/package.json",
        &manifest("@acme/core", "3.1.0-ABC-2", &[]),
    );
    root.write(
        "apps/web/node_modules/@acme/ui/package.json",
        &manifest("@acme/ui", "1.0.0", &[]),
    );
    root.write(
        "apps/admin/package.json",
        &manifest(
            "admin",
            "1.0.0",
            &[("@acme/core", "~3.0.0"), ("@acme/ui", "^1.0.0")],
        ),
    );
    root.write(
        "apps/admin/node_modules/@acme/core/package.json",
        &manifest("@acme/core", "3.1.0", &[]),
    );
    root.write(
        "apps/admin/node_modules/@acme/ui/package.json",
        &manifest("@acme/ui", "1.0.0", &[]),
    );
    let workspace = Workspace::open(root.path()).unwrap();
    let config = config_of(&[
        ("core", "libs/core"),
        ("ui", "libs/ui"),
        ("web", "apps/web"),
        ("admin", "apps/admin"),
    ]);

    let drift = read_dependency_drift(&workspace, &config);

    let core = drift
        .libraries
        .iter()
        .find(|l| l.name == "@acme/core")
        .unwrap();
    let flags: Vec<(&str, bool, bool)> = core
        .cells
        .iter()
        .map(|cell| (cell.project.as_str(), cell.out_of_range, cell.differs))
        .collect();
    assert_eq!(flags, vec![("web", false, true), ("admin", true, true)]);
    assert!(core.mismatch);
    assert_eq!(core.newest_installed.as_deref(), Some("3.1.0"));

    let ui = drift
        .libraries
        .iter()
        .find(|l| l.name == "@acme/ui")
        .unwrap();
    assert!(ui
        .cells
        .iter()
        .all(|cell| !cell.out_of_range && !cell.differs));
    assert!(!ui.mismatch);
}

fn latest(name: &str, version: &str) -> LatestVersion {
    LatestVersion {
        name: name.to_string(),
        version: Some(version.to_string()),
        error: None,
    }
}

#[test]
fn latest_versions_mark_installed_versions_behind_with_prerelease_aware_comparison() {
    let root = TempDir::new("drift-latest");
    let consumers = [
        ("a", "3.1.0-ABC-2"),
        ("b", "3.1.0"),
        ("c", "3.2.0-ABC-1"),
        ("d", "3.0.9"),
    ];
    for (name, installed) in consumers {
        root.write(
            &format!("apps/{name}/package.json"),
            &manifest(
                name,
                "1.0.0",
                &[("@acme/core", "*"), ("left-pad", "^1.0.0")],
            ),
        );
        root.write(
            &format!("apps/{name}/node_modules/@acme/core/package.json"),
            &manifest("@acme/core", installed, &[]),
        );
    }
    let workspace = Workspace::open(root.path()).unwrap();
    let mut config = config_of(&[
        ("a", "apps/a"),
        ("b", "apps/b"),
        ("c", "apps/c"),
        ("d", "apps/d"),
    ]);
    config.packages = vec![
        PackageSpec::new("@acme/core", "3.1.0"),
        PackageSpec::new("left-pad", "1.0.0"),
    ];
    let mut drift = read_dependency_drift(&workspace, &config);

    apply_latest_versions(
        &mut drift,
        &[
            latest("@acme/core", "3.1.0"),
            LatestVersion {
                name: "left-pad".to_string(),
                version: None,
                error: Some("npm error code E404".to_string()),
            },
        ],
    );

    let core = drift
        .libraries
        .iter()
        .find(|l| l.name == "@acme/core")
        .unwrap();
    assert_eq!(core.latest.as_deref(), Some("3.1.0"));
    let behind: Vec<(&str, bool)> = core
        .cells
        .iter()
        .map(|cell| (cell.project.as_str(), cell.behind))
        .collect();
    assert_eq!(
        behind,
        vec![("a", true), ("b", false), ("c", false), ("d", true)]
    );
    let pad = drift
        .libraries
        .iter()
        .find(|l| l.name == "left-pad")
        .unwrap();
    assert_eq!(pad.latest, None);
    assert_eq!(pad.latest_error.as_deref(), Some("npm error code E404"));
    assert!(pad.cells.iter().all(|cell| !cell.behind));
}

#[test]
fn drift_compares_installed_versions_by_npm_semver_validity_and_limits() {
    let root = TempDir::new("drift-npm-semver");
    let consumers = [
        ("big", "900719925474100.0.0", "^1.0.0"),
        ("largest", "9007199254740991.0.0", ">=1"),
        ("garbage", "1.2.3garbage", "*"),
        ("zero", "1.2.3-01", "*"),
        ("equals", "=1.2.3", "*"),
        ("junk", "1.2.3!junk", "*"),
        ("prefixed", "v1.2.3", "^1.2.0"),
        ("padded", " 1.2.3 ", "~1.2.0"),
    ];
    for (name, installed, declared) in consumers {
        root.write(
            &format!("apps/{name}/package.json"),
            &manifest(name, "1.0.0", &[("@acme/core", declared)]),
        );
        root.write(
            &format!("apps/{name}/node_modules/@acme/core/package.json"),
            &manifest("@acme/core", installed, &[]),
        );
    }
    let paths: Vec<(String, String)> = consumers
        .iter()
        .map(|(name, _, _)| (name.to_string(), format!("apps/{name}")))
        .collect();
    let projects: Vec<(&str, &str)> = paths
        .iter()
        .map(|(name, path)| (name.as_str(), path.as_str()))
        .collect();
    let mut config = config_of(&projects);
    config.packages = vec![PackageSpec::new("@acme/core", "1.0.0")];
    let workspace = Workspace::open(root.path()).unwrap();
    let mut drift = read_dependency_drift(&workspace, &config);

    apply_latest_versions(&mut drift, &[latest("@acme/core", "900719925474101.0.0")]);

    let core = &drift.libraries[0];
    assert_eq!(
        core.newest_installed.as_deref(),
        Some("9007199254740991.0.0")
    );
    let flags: Vec<(&str, bool, bool)> = core
        .cells
        .iter()
        .map(|cell| (cell.project.as_str(), cell.out_of_range, cell.behind))
        .collect();
    assert_eq!(
        flags,
        vec![
            ("big", true, true),
            ("largest", false, false),
            ("garbage", false, false),
            ("zero", false, false),
            ("equals", false, false),
            ("junk", false, false),
            ("prefixed", false, true),
            ("padded", false, true),
        ]
    );
}

fn manifest_with_sections(name: &str, sections: &[(&str, &str)]) -> String {
    let fields: Vec<String> = sections
        .iter()
        .map(|(field, entries)| format!("\"{field}\": {{{entries}}}"))
        .collect();
    format!(
        "{{\"name\": \"{name}\", \"version\": \"1.0.0\"{}{}}}",
        if fields.is_empty() { "" } else { ", " },
        fields.join(", ")
    )
}

#[test]
fn graph_lists_workspace_edges_by_section_in_the_run_order_and_reports_cycles() {
    let root = TempDir::new("graph");
    root.write(
        "web/package.json",
        &manifest_with_sections(
            "web",
            &[
                (
                    "dependencies",
                    "\"@acme/ui\": \"^1.0.0\", \"react\": \"^18.0.0\"",
                ),
                ("devDependencies", "\"@acme/core\": \"^1.0.0\""),
            ],
        ),
    );
    root.write(
        "ui/package.json",
        &manifest_with_sections(
            "@acme/ui",
            &[("dependencies", "\"@acme/core\": \"^1.0.0\"")],
        ),
    );
    root.write(
        "core/package.json",
        &manifest_with_sections("@acme/core", &[]),
    );
    root.write(
        "admin/package.json",
        &manifest_with_sections(
            "admin",
            &[("peerDependencies", "\"@acme/core\": \"^1.0.0\"")],
        ),
    );
    root.write(
        "x/package.json",
        &manifest_with_sections("x", &[("dependencies", "\"y\": \"1.0.0\"")]),
    );
    root.write(
        "y/package.json",
        &manifest_with_sections("y", &[("dependencies", "\"x\": \"1.0.0\"")]),
    );
    let workspace = Workspace::open(root.path()).unwrap();
    let config = config_of(&[
        ("web", "web"),
        ("ui", "ui"),
        ("core", "core"),
        ("admin", "admin"),
        ("x", "x"),
        ("y", "y"),
    ]);

    let graph = read_dependency_graph(&workspace, &config);

    let plan = RunPlan {
        project_names: config.projects.iter().map(|p| p.name.clone()).collect(),
        packages: Vec::new(),
        steps: vec![StepName::Build],
        mode: ExecutionMode::PerProject,
        concurrency: 1,
        dry_run: true,
        extra_install_args: Vec::new(),
        label: "order".to_string(),
        save_as: None,
        version: VersionPolicy::default(),
        project_bumps: Default::default(),
    };
    let run_order: Vec<String> = build_jobs(&workspace, &config, &plan)
        .jobs
        .into_iter()
        .map(|job| job.name)
        .collect();
    let graph_order: Vec<String> = graph.nodes.iter().map(|node| node.name.clone()).collect();
    assert_eq!(graph_order, run_order);
    assert_eq!(graph_order, vec!["core", "admin", "ui", "web", "x", "y"]);

    let layers: Vec<(&str, u32)> = graph
        .nodes
        .iter()
        .map(|node| (node.name.as_str(), node.layer))
        .collect();
    assert_eq!(
        layers,
        vec![
            ("core", 0),
            ("admin", 1),
            ("ui", 1),
            ("web", 2),
            ("x", 0),
            ("y", 1)
        ]
    );

    let edges: Vec<(&str, &str, DependencySection)> = graph
        .edges
        .iter()
        .map(|edge| (edge.consumer.as_str(), edge.library.as_str(), edge.section))
        .collect();
    assert_eq!(
        edges,
        vec![
            ("web", "ui", DependencySection::Dependencies),
            ("web", "core", DependencySection::Dev),
            ("ui", "core", DependencySection::Dependencies),
            ("admin", "core", DependencySection::Peer),
            ("x", "y", DependencySection::Dependencies),
            ("y", "x", DependencySection::Dependencies),
        ]
    );
    assert_eq!(graph.cycles, vec![vec!["x".to_string(), "y".to_string()]]);
}

fn copies_fixture(label: &str) -> (TempDir, UpdateConfig) {
    let root = TempDir::new(label);
    root.write(
        "libs/core/package.json",
        &manifest("@acme/core", "3.1.0", &[]),
    );
    root.write("libs/ui/package.json", &manifest("@acme/ui", "1.0.0", &[]));
    root.write(
        "apps/web/package.json",
        &manifest(
            "web",
            "1.0.0",
            &[
                ("@acme/core", "^3.0.0"),
                ("@acme/ui", "^1.0.0"),
                ("lodash", "^4.0.0"),
            ],
        ),
    );
    let installed = [
        ("node_modules/@acme/core", "@acme/core", "3.1.0"),
        ("node_modules/@acme/ui", "@acme/ui", "1.0.0"),
        (
            "node_modules/@acme/ui/node_modules/@acme/core",
            "@acme/core",
            "3.0.0",
        ),
        ("node_modules/legacy", "legacy", "1.0.0"),
        ("node_modules/legacy/node_modules/x", "x", "1.0.0"),
        (
            "node_modules/legacy/node_modules/x/node_modules/@acme/core",
            "@acme/core",
            "2.0.0",
        ),
        ("node_modules/lodash", "lodash", "4.17.21"),
        (
            "node_modules/legacy/node_modules/lodash",
            "lodash",
            "3.10.1",
        ),
    ];
    for (path, name, version) in installed {
        root.write(
            &format!("apps/web/{path}/package.json"),
            &manifest(name, version, &[]),
        );
    }
    root.write("apps/web/node_modules/.bin/tool", "");
    root.write("apps/web/node_modules/.package-lock.json", "{}");
    root.write(
        "apps/admin/package.json",
        &manifest("admin", "1.0.0", &[("@acme/core", "^3.0.0")]),
    );
    let config = config_of(&[
        ("core", "libs/core"),
        ("ui", "libs/ui"),
        ("web", "apps/web"),
        ("admin", "apps/admin"),
        ("gone", "apps/gone"),
    ]);
    (root, config)
}

type CopyRows = Vec<(String, Vec<(String, String)>)>;

fn copies_summary(copies: &[ProjectCopies]) -> Vec<(String, bool, CopyRows)> {
    copies
        .iter()
        .map(|project| {
            (
                project.project.clone(),
                project.has_node_modules,
                project
                    .packages
                    .iter()
                    .map(|package| {
                        (
                            package.name.clone(),
                            package
                                .copies
                                .iter()
                                .map(|copy| {
                                    (copy.version.clone().unwrap_or_default(), copy.path.clone())
                                })
                                .collect(),
                        )
                    })
                    .collect(),
            )
        })
        .collect()
}

#[test]
fn copies_lists_libraries_installed_more_than_once_with_version_and_relative_path() {
    let (root, config) = copies_fixture("copies-libraries");
    let workspace = Workspace::open(root.path()).unwrap();

    let copies = find_installed_copies(
        &workspace,
        &config,
        CopyScope::Libraries,
        &DependencyRequest::new(),
    );

    let core_copies = vec![
        ("3.1.0".to_string(), "node_modules/@acme/core".to_string()),
        (
            "3.0.0".to_string(),
            "node_modules/@acme/ui/node_modules/@acme/core".to_string(),
        ),
        (
            "2.0.0".to_string(),
            "node_modules/legacy/node_modules/x/node_modules/@acme/core".to_string(),
        ),
    ];
    assert_eq!(
        copies_summary(&copies),
        vec![
            ("core".to_string(), false, vec![]),
            ("ui".to_string(), false, vec![]),
            (
                "web".to_string(),
                true,
                vec![("@acme/core".to_string(), core_copies)]
            ),
            ("admin".to_string(), false, vec![]),
        ]
    );
}

#[test]
fn copies_of_all_packages_include_packages_that_are_not_libraries() {
    let (root, config) = copies_fixture("copies-all");
    let workspace = Workspace::open(root.path()).unwrap();

    let copies = find_installed_copies(
        &workspace,
        &config,
        CopyScope::AllPackages,
        &DependencyRequest::new(),
    );

    let web = copies
        .iter()
        .find(|project| project.project == "web")
        .unwrap();
    let names: Vec<&str> = web.packages.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["@acme/core", "lodash"]);
    let lodash: Vec<(Option<&str>, &str)> = web.packages[1]
        .copies
        .iter()
        .map(|copy| (copy.version.as_deref(), copy.path.as_str()))
        .collect();
    assert_eq!(
        lodash,
        vec![
            (Some("4.17.21"), "node_modules/lodash"),
            (Some("3.10.1"), "node_modules/legacy/node_modules/lodash"),
        ]
    );
}

#[test]
fn copies_scan_survives_links_back_into_the_project_and_reads_linked_project_folders() {
    let (root, mut config) = copies_fixture("copies-links");
    link_folder(
        &root.join("apps/web/node_modules/web-self"),
        &root.join("apps/web"),
    );
    root.mkdir("apps/web/node_modules/legacy/node_modules/@acme");
    link_folder(
        &root.join("apps/web/node_modules/legacy/node_modules/@acme/ui"),
        &root.join("libs/ui"),
    );
    link_folder(&root.join("linked-web"), &root.join("apps/web"));
    config
        .projects
        .push(ConfiguredProject::new("linked", "linked-web"));
    let workspace = Workspace::open(root.path()).unwrap();

    let copies = find_installed_copies(
        &workspace,
        &config,
        CopyScope::Libraries,
        &DependencyRequest::new(),
    );

    let expected = vec![
        (
            "@acme/core".to_string(),
            vec![
                ("3.1.0".to_string(), "node_modules/@acme/core".to_string()),
                (
                    "3.0.0".to_string(),
                    "node_modules/@acme/ui/node_modules/@acme/core".to_string(),
                ),
                (
                    "2.0.0".to_string(),
                    "node_modules/legacy/node_modules/x/node_modules/@acme/core".to_string(),
                ),
            ],
        ),
        (
            "@acme/ui".to_string(),
            vec![
                ("1.0.0".to_string(), "node_modules/@acme/ui".to_string()),
                (
                    "1.0.0".to_string(),
                    "node_modules/legacy/node_modules/@acme/ui".to_string(),
                ),
            ],
        ),
    ];
    let summary = copies_summary(&copies);
    let web = summary.iter().find(|(name, _, _)| name == "web").unwrap();
    assert_eq!(web.2, expected);
    let linked = summary
        .iter()
        .find(|(name, _, _)| name == "linked")
        .unwrap();
    assert_eq!(linked.2, expected);
}

#[test]
fn copies_scan_enters_each_node_modules_folder_once_when_a_link_leads_back_to_it() {
    let root = TempDir::new("copies-node-modules-link");
    root.write(
        "app/package.json",
        &manifest("app", "1.0.0", &[("@acme/core", "*"), ("host", "*")]),
    );
    root.write(
        "app/node_modules/@acme/core/package.json",
        &manifest("@acme/core", "1.0.0", &[]),
    );
    root.write(
        "app/node_modules/host/package.json",
        &manifest("host", "1.0.0", &[]),
    );
    root.mkdir("app/node_modules/host/node_modules/inner");
    link_folder(
        &root.join("app/node_modules/host/node_modules/inner/node_modules"),
        &root.join("app/node_modules"),
    );
    let mut config = config_of(&[("app", "app")]);
    config.packages = vec![PackageSpec::new("@acme/core", "1.0.0")];
    let workspace = Workspace::open(root.path()).unwrap();

    let libraries = find_installed_copies(
        &workspace,
        &config,
        CopyScope::Libraries,
        &DependencyRequest::new(),
    );
    let everything = find_installed_copies(
        &workspace,
        &config,
        CopyScope::AllPackages,
        &DependencyRequest::new(),
    );

    assert_eq!(
        copies_summary(&libraries),
        vec![("app".to_string(), true, vec![])]
    );
    assert_eq!(
        copies_summary(&everything),
        vec![("app".to_string(), true, vec![])]
    );
}

#[test]
fn a_package_name_two_projects_share_has_no_provider_and_is_shown_as_ambiguous() {
    let root = TempDir::new("drift-ambiguous");
    root.write(
        "libs/core/package.json",
        &manifest("@acme/core", "3.2.0", &[]),
    );
    root.write(
        "copy/core/package.json",
        &manifest("@acme/core", "3.3.0", &[]),
    );
    root.write(
        "apps/web/package.json",
        &manifest("web", "1.0.0", &[("@acme/core", "^3.1.0")]),
    );
    let workspace = Workspace::open(root.path()).unwrap();
    let config = config_of(&[
        ("core", "libs/core"),
        ("core-copy", "copy/core"),
        ("web", "apps/web"),
    ]);

    let drift = read_dependency_drift(&workspace, &config);
    let graph = read_dependency_graph(&workspace, &config);

    let core = &drift.libraries[0];
    assert_eq!(
        (
            core.name.as_str(),
            core.local_project.as_deref(),
            core.local_version.as_deref(),
            core.ambiguous_projects.clone()
        ),
        (
            "@acme/core",
            None,
            None,
            vec!["core".to_string(), "core-copy".to_string()]
        )
    );
    assert!(graph.edges.is_empty(), "{:?}", graph.edges);
    let web = graph.nodes.iter().find(|node| node.name == "web").unwrap();
    assert!(web.depends_on.is_empty(), "{:?}", web.depends_on);
}
