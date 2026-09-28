mod common;

use common::TempDir;
use deptide_core::domain::{
    BumpWhen, ExecutionMode, PackageSpec, SavedRun, Settings, StepName, VersionBump, VersionPolicy,
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

fn parse_policy(text: &str) -> VersionPolicy {
    serde_json::from_str(text).unwrap_or_else(|error| panic!("{text} does not parse: {error}"))
}

#[test]
fn version_policies_saved_before_the_bump_condition_existed_keep_their_meaning() {
    assert_eq!(
        parse_policy(r#"{"bump":"minor","onlyIfSameAsMain":true}"#),
        VersionPolicy {
            bump: VersionBump::Minor,
            when: BumpWhen::SameAsMain,
        }
    );
    assert_eq!(
        parse_policy(r#"{"bump":"minor","onlyIfSameAsMain":false}"#),
        VersionPolicy {
            bump: VersionBump::Minor,
            when: BumpWhen::Always,
        }
    );
    assert_eq!(
        parse_policy(r#"{"bump":"major"}"#),
        VersionPolicy {
            bump: VersionBump::Major,
            when: BumpWhen::Always,
        }
    );
    assert_eq!(parse_policy("{}"), VersionPolicy::default());
}

#[test]
fn the_bump_condition_wins_over_the_old_flag() {
    assert_eq!(
        parse_policy(r#"{"bump":"patch","when":"not-bumped-on-branch","onlyIfSameAsMain":true}"#)
            .when,
        BumpWhen::NotBumpedOnBranch
    );
    assert_eq!(
        parse_policy(r#"{"bump":"patch","when":"always","onlyIfSameAsMain":true}"#).when,
        BumpWhen::Always
    );
}

#[test]
fn version_policies_are_written_with_the_bump_condition_only() {
    for (when, text) in [
        (BumpWhen::Always, "always"),
        (BumpWhen::SameAsMain, "same-as-main"),
        (BumpWhen::NotBumpedOnBranch, "not-bumped-on-branch"),
    ] {
        let policy = VersionPolicy {
            bump: VersionBump::Minor,
            when,
        };
        let json = serde_json::to_value(policy).unwrap();

        assert_eq!(json, serde_json::json!({ "bump": "minor", "when": text }));
        assert_eq!(
            serde_json::from_value::<VersionPolicy>(json).unwrap(),
            policy
        );
    }
}

#[test]
fn a_saved_run_file_from_before_the_bump_condition_still_loads() {
    let root = TempDir::new("config-old-run");
    let workspace = Workspace::open(root.path()).expect("workspace");
    std::fs::create_dir_all(workspace.runs_directory()).unwrap();
    std::fs::write(
        workspace.runs_directory().join("old.json"),
        r#"{
          "name": "old",
          "savedAt": "2026-01-01T00:00:00Z",
          "projects": ["web"],
          "packages": [{ "name": "@acme/core", "version": "1.0.0", "saveDev": false }],
          "steps": ["install", "version"],
          "concurrency": 2,
          "version": { "bump": "minor", "onlyIfSameAsMain": true }
        }"#,
    )
    .unwrap();

    let run = load_run(&workspace, "old").expect("the old run is listed");

    assert_eq!(
        run.version,
        VersionPolicy {
            bump: VersionBump::Minor,
            when: BumpWhen::SameAsMain,
        }
    );
}

#[test]
fn a_saved_run_from_before_peer_installs_loads_its_packages_as_non_peer() {
    let root = TempDir::new("config-old-peer");
    let workspace = Workspace::open(root.path()).expect("workspace");
    std::fs::create_dir_all(workspace.runs_directory()).unwrap();
    std::fs::write(
        workspace.runs_directory().join("old.json"),
        r#"{
          "name": "old",
          "savedAt": "2026-01-01T00:00:00Z",
          "projects": ["web"],
          "packages": [{ "name": "@acme/core", "version": "1.0.0", "saveDev": true }],
          "steps": ["install"],
          "concurrency": 2
        }"#,
    )
    .unwrap();

    let run = load_run(&workspace, "old").expect("the old run is listed");
    assert!(run.packages[0].save_dev);
    assert!(!run.packages[0].save_peer);
}

#[test]
fn a_peer_package_round_trips_through_json_as_save_peer() {
    let peer = PackageSpec {
        save_peer: true,
        ..PackageSpec::new("a", "1.0.0")
    };
    let json = serde_json::to_value(&peer).unwrap();
    assert_eq!(json["savePeer"], true);
    assert_eq!(serde_json::from_value::<PackageSpec>(json).unwrap(), peer);
}

#[test]
fn a_config_package_can_ask_for_a_peer_install() {
    let config = parse_config_text(
        r#"{"packages":[{ "name": "@acme/ui", "version": "9.0.0", "savePeer": true }]}"#,
    )
    .unwrap();
    assert!(config.packages[0].save_peer);
}
