mod common;

use common::{manifest, TempDir};
use deptide_core::domain::{ConfiguredProject, UpdateConfig};
use deptide_core::scan::{
    apply_scan_selection, apply_suffix, collect_dependency_candidates, detect_projects,
    find_duplicate_groups, propose_name, read_project_at, strip_prerelease, DetectionOptions,
};
use deptide_core::workspace::Workspace;

fn options(depth: u32) -> DetectionOptions {
    DetectionOptions::new(depth, &["skip-me".to_string()])
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

#[test]
fn the_workspace_own_folders_are_never_detected_as_projects() {
    let root = TempDir::new("scan-internal");
    root.write("apps/web/package.json", &manifest("web", "1.0.0", &[]));
    root.write(
        "tool/backups/web-2026-01-01/package.json",
        &manifest("web", "0.9.0", &[]),
    );
    root.write(
        "tool/transfers/web/package.json",
        &manifest("web", "0.9.0", &[]),
    );
    let workspace = Workspace::open(root.join("tool")).unwrap();

    let plain = detect_projects(root.path(), &options(4));
    assert_eq!(plain.len(), 3, "without exclusions the copies are found");

    let options = options(4).excluding(workspace.internal_directories());
    let found = detect_projects(root.path(), &options);
    let names: Vec<&str> = found.iter().map(|p| p.relative_path.as_str()).collect();
    assert_eq!(names, vec!["apps/web"]);
}

fn consumers_with_sections() -> Vec<deptide_core::domain::DetectedProject> {
    let root = TempDir::new("scan-sections");
    root.write(
        "web/package.json",
        r#"{"name":"web","version":"1.0.0",
            "dependencies":{"shared":"^1.0.0"},
            "devDependencies":{"tooling":"^2.1.0","dev-only":"^1.0.0"},
            "peerDependencies":{"tooling":"^2.0.0","peer-only":"^1.0.0"}}"#,
    );
    root.write(
        "api/package.json",
        r#"{"name":"api","version":"1.0.0",
            "devDependencies":{"shared":"^1.1.0","dev-only":"^1.2.0","mixed":"^3.0.0"},
            "peerDependencies":{"shared":"^1.0.0"},
            "dependencies":{"mixed":"^3.0.0"}}"#,
    );

    detect_projects(root.path(), &options(2))
}

fn candidate<'a>(
    candidates: &'a [deptide_core::domain::DependencyCandidate],
    name: &str,
) -> &'a deptide_core::domain::DependencyCandidate {
    candidates
        .iter()
        .find(|candidate| candidate.name == name)
        .unwrap_or_else(|| panic!("{name} is a candidate"))
}

#[test]
fn candidates_list_every_section_once_in_precedence_order() {
    use deptide_core::domain::DependencySection::{Dependencies, Dev, Peer};

    let consumers = consumers_with_sections();
    let candidates = collect_dependency_candidates(&consumers, &consumers, "");

    assert_eq!(
        candidate(&candidates, "shared").sections,
        vec![Dependencies, Peer, Dev]
    );
    assert_eq!(candidate(&candidates, "tooling").sections, vec![Peer, Dev]);
    assert_eq!(candidate(&candidates, "dev-only").sections, vec![Dev]);
    assert_eq!(
        candidate(&candidates, "mixed").sections,
        vec![Dependencies, Dev]
    );
}

#[test]
fn a_peer_only_package_is_a_candidate_with_its_range_and_consumer() {
    use deptide_core::domain::DependencySection::Peer;

    let consumers = consumers_with_sections();
    let candidates = collect_dependency_candidates(&consumers, &consumers, "");
    let peer_only = candidate(&candidates, "peer-only");

    assert_eq!(peer_only.sections, vec![Peer]);
    assert_eq!(peer_only.current_ranges, vec!["1.0.0"]);
    assert_eq!(peer_only.used_by.len(), 1);
    assert!(!peer_only.is_dev_dependency);
}

#[test]
fn candidates_without_local_libraries_are_ordered_by_name() {
    let consumers = consumers_with_sections();
    let candidates = collect_dependency_candidates(&consumers, &consumers, "");

    let names: Vec<&str> = candidates.iter().map(|c| c.name.as_str()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(
        names, sorted,
        "without local libraries the order is by name"
    );
}

#[test]
fn a_candidate_is_dev_only_when_every_consumer_has_it_as_dev_and_not_as_dependency() {
    let consumers = consumers_with_sections();
    let candidates = collect_dependency_candidates(&consumers, &consumers, "");

    assert!(candidate(&candidates, "dev-only").is_dev_dependency);
    assert!(
        candidate(&candidates, "tooling").is_dev_dependency,
        "a peer that is also a dev dependency installs with --save-dev"
    );
    assert!(!candidate(&candidates, "mixed").is_dev_dependency);
    assert!(!candidate(&candidates, "shared").is_dev_dependency);
}

#[test]
fn a_detected_project_saved_without_peer_dependencies_still_loads() {
    let project: deptide_core::domain::DetectedProject = serde_json::from_str(
        r#"{"directory":"C:/repos/web","relativePath":"web","proposedName":"web",
            "packageName":"web","version":"1.0.0","dependencies":{"vue":"^3.5.0"},
            "devDependencies":{},"hasBuildScript":true}"#,
    )
    .expect("old shape deserializes");

    assert!(project.peer_dependencies.is_empty());
    assert_eq!(project.dependencies.len(), 1);
}
