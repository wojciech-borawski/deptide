mod common;

use common::TempDir;
use deptide_core::domain::{
    ExecutionMode, PackageSpec, SavedRun, Settings, StepName, VersionPolicy,
};
use deptide_core::workspace::{
    delete_run, list_runs, load_config, load_run, load_settings, parse_config_text, save_config,
    save_run, save_settings, suggest_run_name, Workspace,
};

#[test]
fn legacy_config_shapes_are_accepted() {
    let text = r#"{
      "auditFix": true,
      "installArgs": ["--no-fund"],
      "packages": ["@acme/core@3.1.0-ABC-1", { "name": "@acme/ui", "version": "9.0.0", "saveDev": true }, { "name": "broken" }],
      "projects": ["../repos/web", { "name": "api", "path": "../repos/api", "packages": ["@acme/core"], "skip": true }, { "name": "no-path" }]
    }"#;

    let config = parse_config_text(text).expect("parses");

    assert_eq!(config.packages.len(), 2);
    assert_eq!(
        config.packages[0],
        PackageSpec::new("@acme/core", "3.1.0-ABC-1")
    );
    assert!(config.packages[1].save_dev);
    assert_eq!(config.projects.len(), 2);
    assert_eq!(config.projects[0].name, "web");
    assert!(config.projects[1].skip);
    assert_eq!(
        config.projects[1].packages.as_deref(),
        Some(&["@acme/core".to_string()][..])
    );
    assert_eq!(config.install_args, vec!["--no-fund"]);
}

#[test]
fn config_round_trips_through_the_workspace() {
    let root = TempDir::new("config");
    let workspace = Workspace::open(root.path()).expect("workspace");

    let mut config = load_config(&workspace).expect("missing file is an empty config");
    assert!(config.projects.is_empty());

    config
        .packages
        .push(PackageSpec::new("@acme/core", "1.0.0"));
    save_config(&workspace, &config).expect("saves");

    let reloaded = load_config(&workspace).expect("loads");
    assert_eq!(reloaded, config);
    assert!(root.join("update-libs.json").exists());
}

#[test]
fn settings_fall_back_to_defaults_and_are_sanitized() {
    let root = TempDir::new("config");
    let workspace = Workspace::open(root.path()).expect("workspace");

    assert_eq!(load_settings(&workspace), Settings::default());

    root.write("settings.json", "{ not json");
    assert_eq!(load_settings(&workspace), Settings::default());

    let saved = save_settings(
        &workspace,
        &Settings {
            projects_root: "  C:/repos  ".to_string(),
            scan_depth: 0,
            concurrency: 0,
            steps: vec![],
            mode: ExecutionMode::PerStep,
            extra_ignored_directories: vec![" tmp ".to_string(), "".to_string(), "tmp".to_string()],
            branch_suffix_pattern: "([".to_string(),
        },
    )
    .expect("saves");

    assert_eq!(saved.projects_root, "C:/repos");
    assert_eq!(saved.scan_depth, Settings::default().scan_depth);
    assert_eq!(saved.concurrency, Settings::default().concurrency);
    assert_eq!(saved.steps, StepName::default_steps().to_vec());
    assert_eq!(saved.mode, ExecutionMode::PerStep);
    assert_eq!(saved.extra_ignored_directories, vec!["tmp"]);
    assert_eq!(
        saved.branch_suffix_pattern,
        Settings::default().branch_suffix_pattern
    );
    assert_eq!(load_settings(&workspace), saved);
}

#[test]
fn saved_runs_are_listed_newest_first_and_can_be_deleted() {
    let root = TempDir::new("config");
    let workspace = Workspace::open(root.path()).expect("workspace");

    let run = |name: &str, saved_at: &str| SavedRun {
        name: name.to_string(),
        saved_at: saved_at.to_string(),
        projects: vec!["web".to_string()],
        packages: vec![PackageSpec::new("@acme/core", "1.0.0")],
        steps: vec![StepName::Install],
        concurrency: 2,
        mode: ExecutionMode::PerProject,
        extra_install_args: vec![],
        version: VersionPolicy::default(),
    };

    save_run(&workspace, &run("Older Run!", "2026-01-01T00:00:00Z")).expect("saves");
    save_run(&workspace, &run("newer", "2026-02-01T00:00:00Z")).expect("saves");

    let listed = list_runs(&workspace);
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].run.name, "newer");
    assert_eq!(listed[1].file_name, "older-run.json");

    assert!(load_run(&workspace, "Older Run!").is_some());
    delete_run(&workspace, "older-run").expect("deletes");
    assert!(load_run(&workspace, "Older Run!").is_none());
    assert!(delete_run(&workspace, "ghost").is_err());
}

#[test]
fn run_names_are_suggested_from_the_first_package() {
    let suggestion = suggest_run_name(&["@acme/core".to_string(), "@acme/ui".to_string()]);
    assert!(suggestion.ends_with("-core-plus-1"));
    assert!(suggest_run_name(&[]).ends_with("-update"));
}
