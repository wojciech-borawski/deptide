mod common;

use common::{manifest, TempDir};
use deptide_core::domain::{ConfiguredProject, UpdateConfig};
use deptide_core::scan::{
    apply_scan_selection, apply_suffix, build_ignored_directories, collect_dependency_candidates,
    detect_projects, find_duplicate_groups, propose_name, read_project_at, strip_prerelease,
    DetectionOptions,
};
use deptide_core::workspace::Workspace;

fn options(depth: u32) -> DetectionOptions {
    DetectionOptions {
        max_depth: depth,
        ignored_directories: build_ignored_directories(&["skip-me".to_string()]),
    }
}

#[test]
fn propose_name_collapses_repeated_segments() {
    assert_eq!(
        propose_name("Shop/Web/Shop-Storefront-App/Storefront-App"),
        "Web-Shop-Storefront-App"
    );
    assert_eq!(
        propose_name("Api/Services/Api-Billing-Service/Billing-Service"),
        "Services-Api-Billing-Service"
    );
    assert_eq!(propose_name("libs/core"), "libs-core");
    assert_eq!(propose_name(""), "root");
}

#[test]
fn detect_projects_finds_manifests_and_respects_depth_and_ignores() {
    let root = TempDir::new("scan");
    root.write(
        "libs/core/package.json",
        &manifest("@acme/core", "3.1.0-ABC-1", &[]),
    );
    root.write(
        "apps/web/package.json",
        &manifest("web", "1.0.0", &[("@acme/core", "^3.0.0")]),
    );
    root.write(
        "apps/web/node_modules/dep/package.json",
        &manifest("dep", "1.0.0", &[]),
    );
    root.write(
        "skip-me/hidden/package.json",
        &manifest("hidden", "1.0.0", &[]),
    );
    root.write("deep/a/b/c/package.json", &manifest("deep", "1.0.0", &[]));

    let found = detect_projects(root.path(), &options(2));
    let names: Vec<&str> = found
        .iter()
        .filter_map(|p| p.package_name.as_deref())
        .collect();

    assert_eq!(names, vec!["web", "@acme/core"]);
    assert!(found
        .iter()
        .all(|project| !project.relative_path.contains("node_modules")));

    let deeper = detect_projects(root.path(), &options(4));
    assert!(deeper
        .iter()
        .any(|p| p.package_name.as_deref() == Some("deep")));
}

#[test]
fn candidates_know_local_versions_and_consumers() {
    let root = TempDir::new("scan");
    root.write(
        "libs/core/package.json",
        &manifest("@acme/core", "3.1.0-ABC-1", &[]),
    );
    root.write(
        "apps/web/package.json",
        &manifest(
            "web",
            "1.0.0",
            &[("@acme/core", "^3.0.0"), ("vue", "^3.5.0")],
        ),
    );
    root.write(
        "apps/api/package.json",
        &manifest("api", "1.0.0", &[("@acme/core", "3.0.9")]),
    );

    let all = detect_projects(root.path(), &options(3));
    let consumers: Vec<_> = all
        .iter()
        .filter(|p| p.package_name.as_deref() != Some("@acme/core"))
        .cloned()
        .collect();

    let candidates = collect_dependency_candidates(&all, &consumers, "");

    assert_eq!(candidates[0].name, "@acme/core");
    assert_eq!(candidates[0].local_version.as_deref(), Some("3.1.0-ABC-1"));
    assert_eq!(candidates[0].current_ranges, vec!["3.0.9", "3.0.0"]);
    assert_eq!(candidates[0].used_by.len(), 2);
    assert_eq!(candidates[1].name, "vue");
    assert!(candidates[1].local_version.is_none());
}

#[test]
fn manifests_are_classified_as_library_or_application() {
    let root = TempDir::new("kind");
    root.write("lib/package.json", r#"{"name":"@acme/core","version":"1.0.0","main":"dist/index.js","types":"dist/index.d.ts"}"#);
    root.write(
        "peer/package.json",
        r#"{"name":"@acme/ui","version":"1.0.0","peerDependencies":{"vue":"^3"}}"#,
    );
    root.write("app/package.json", r#"{"name":"storefront","version":"1.0.0","private":true,"scripts":{"dev":"vite"},"dependencies":{"vue":"^3"}}"#);

    let kind = |path: &str| read_project_at(&root.join(path), root.path()).unwrap().kind;
    assert_eq!(kind("lib"), deptide_core::domain::ProjectKind::Library);
    assert_eq!(kind("peer"), deptide_core::domain::ProjectKind::Library);
    assert_eq!(kind("app"), deptide_core::domain::ProjectKind::Application);
}

#[test]
fn version_helpers_strip_prerelease_and_apply_suffix() {
    assert_eq!(strip_prerelease("3.1.0-ABC-123"), "3.1.0");
    assert_eq!(strip_prerelease(" 1.2.3 "), "1.2.3");
    assert_eq!(strip_prerelease("latest"), "latest");
    assert_eq!(apply_suffix("3.1.0-old", "-ABC-1"), "3.1.0-ABC-1");
    assert_eq!(apply_suffix("3.1.0", "  "), "3.1.0");
}

#[test]
fn apply_scan_selection_adds_removes_and_keeps_unrelated_entries() {
    let root = TempDir::new("scan");
    let tool = root.mkdir("tool");
    root.write("repos/web/package.json", &manifest("web", "1.0.0", &[]));
    root.write("repos/api/package.json", &manifest("api", "1.0.0", &[]));
    root.write("repos/old/package.json", &manifest("old", "1.0.0", &[]));

    let workspace = Workspace::open(&tool).expect("workspace");
    let config = UpdateConfig {
        projects: vec![
            ConfiguredProject::new("old", "../repos/old"),
            ConfiguredProject::new("elsewhere", "../../somewhere/else"),
        ],
        ..UpdateConfig::default()
    };

    let offered = detect_projects(&root.join("repos"), &options(2));
    let selected: Vec<String> = offered
        .iter()
        .filter(|p| p.package_name.as_deref() != Some("old"))
        .map(|p| p.directory.clone())
        .collect();

    let outcome = apply_scan_selection(&workspace, &config, &offered, &selected);
    let names: Vec<&str> = outcome
        .config
        .projects
        .iter()
        .map(|p| p.name.as_str())
        .collect();

    assert_eq!(outcome.removed, vec!["old"]);
    assert_eq!(outcome.added.len(), 2);
    assert!(names.contains(&"elsewhere"));
    assert!(outcome
        .config
        .projects
        .iter()
        .all(|p| p.path.starts_with("../")));
}

#[test]
fn duplicate_groups_use_the_resolved_folder() {
    let root = TempDir::new("scan");
    let tool = root.mkdir("tool");
    root.mkdir("repos/web");

    let workspace = Workspace::open(&tool).expect("workspace");
    let projects = vec![
        ConfiguredProject::new("web", "../repos/web"),
        ConfiguredProject::new("web-again", "../repos/./web/"),
        ConfiguredProject::new("api", "../repos/api"),
    ];

    let groups = find_duplicate_groups(&projects, &workspace);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].entries, vec!["web", "web-again"]);
}
