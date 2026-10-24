mod common;

use common::TempDir;
use deptide_core::domain::{
    AuditCounts, BumpWhen, ConfiguredProject, ExecutionMode, PackMode, PackageSpec, ProjectGroup,
    SavedRun, Settings, StepName, UpdateConfig, VersionBump, VersionPolicy,
};
use deptide_core::execution::list_summaries;
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
fn project_groups_round_trip_through_update_libs_json() {
    let root = TempDir::new("config-groups");
    let workspace = Workspace::open(root.path()).expect("workspace");
    let config = UpdateConfig {
        projects: vec![
            ConfiguredProject::new("web", "../web"),
            ConfiguredProject::new("api", "../api"),
        ],
        groups: vec![ProjectGroup {
            name: "frontend apps".to_string(),
            projects: vec!["web".to_string()],
        }],
        ..UpdateConfig::default()
    };

    save_config(&workspace, &config).expect("saves");
    let reloaded = load_config(&workspace).expect("loads");

    assert_eq!(reloaded.groups, config.groups);
}

#[test]
fn a_config_without_groups_is_written_without_a_groups_key() {
    let root = TempDir::new("config-no-groups");
    let workspace = Workspace::open(root.path()).expect("workspace");
    let config = UpdateConfig {
        projects: vec![ConfiguredProject::new("web", "../web")],
        ..UpdateConfig::default()
    };

    save_config(&workspace, &config).expect("saves");
    let text = std::fs::read_to_string(root.join("update-libs.json")).unwrap();

    assert!(!text.contains("groups"), "{text}");
}

#[test]
fn saving_drops_group_members_that_are_no_longer_configured() {
    let root = TempDir::new("config-group-prune");
    let workspace = Workspace::open(root.path()).expect("workspace");
    let group = |name: &str, projects: &[&str]| ProjectGroup {
        name: name.to_string(),
        projects: projects.iter().map(|project| project.to_string()).collect(),
    };
    let config = UpdateConfig {
        projects: vec![
            ConfiguredProject::new("web", "../web"),
            ConfiguredProject::new("api", "../api"),
        ],
        groups: vec![
            group("frontend", &["web", "removed-app"]),
            group("gone", &["removed-app"]),
            group("all", &["api", "web"]),
        ],
        ..UpdateConfig::default()
    };

    save_config(&workspace, &config).expect("saves");
    let reloaded = load_config(&workspace).expect("loads");

    assert_eq!(
        reloaded.groups,
        vec![group("frontend", &["web"]), group("all", &["api", "web"])]
    );
}

#[test]
fn saving_keeps_top_level_keys_the_app_does_not_know() {
    let root = TempDir::new("config-unknown-keys");
    root.write(
        "update-libs.json",
        r#"{
  "$schema": "./update-libs.schema.json",
  "auditFix": true,
  "projects": [{ "name": "web", "path": "../web" }],
  "notes": { "owner": "team a" }
}"#,
    );
    let workspace = Workspace::open(root.path()).expect("workspace");
    let mut config = load_config(&workspace).expect("loads");
    config.groups.push(ProjectGroup {
        name: "frontend".to_string(),
        projects: vec!["web".to_string()],
    });

    save_config(&workspace, &config).expect("saves");
    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("update-libs.json")).unwrap())
            .unwrap();

    assert_eq!(written["$schema"], "./update-libs.schema.json");
    assert_eq!(written["auditFix"], true);
    assert_eq!(written["notes"]["owner"], "team a");
    assert_eq!(written["groups"][0]["name"], "frontend");
}

#[test]
fn saving_keeps_fields_the_app_does_not_know_inside_project_and_package_entries() {
    let root = TempDir::new("config-unknown-entry-fields");
    root.write(
        "update-libs.json",
        r#"{
  "packages": [{ "name": "left-pad", "version": "1.3.0", "reason": "pinned for ie11" }],
  "projects": [
    { "name": "web", "path": "../web", "skip": true, "owner": "team a" },
    { "path": "../api", "notes": ["no tests"] }
  ]
}"#,
    );
    let workspace = Workspace::open(root.path()).expect("workspace");
    let mut config = load_config(&workspace).expect("loads");
    config.projects[0].skip = false;

    save_config(&workspace, &config).expect("saves");
    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("update-libs.json")).unwrap())
            .unwrap();

    assert_eq!(written["packages"][0]["reason"], "pinned for ie11");
    assert_eq!(written["projects"][0]["owner"], "team a");
    assert_eq!(written["projects"][0].get("skip"), None);
    assert_eq!(written["projects"][1]["notes"][0], "no tests");
}

#[test]
fn nameless_projects_in_folders_of_the_same_name_keep_their_own_unknown_fields() {
    let root = TempDir::new("config-unknown-nameless");
    root.write(
        "update-libs.json",
        r#"{
  "projects": [
    { "path": "../team-a/api", "owner": "A", "extraA": true },
    { "path": "../team-b/api", "owner": "B", "extraB": true },
    { "name": "web", "path": "../one/web", "owner": "one" },
    { "name": "web", "path": "../two/web", "owner": "two" }
  ]
}"#,
    );
    let workspace = Workspace::open(root.path()).expect("workspace");
    let config = load_config(&workspace).expect("loads");

    save_config(&workspace, &config).expect("saves");
    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("update-libs.json")).unwrap())
            .unwrap();
    let field = |index: usize, key: &str| written["projects"][index].get(key).cloned();

    assert_eq!(
        [field(0, "owner"), field(0, "extraA"), field(0, "extraB")],
        [Some("A".into()), Some(true.into()), None]
    );
    assert_eq!(
        [field(1, "owner"), field(1, "extraA"), field(1, "extraB")],
        [Some("B".into()), None, Some(true.into())]
    );
    assert_eq!(
        [field(2, "owner"), field(3, "owner")],
        [Some("one".into()), Some("two".into())]
    );
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
        project_bumps: [("web".to_string(), VersionBump::Minor)].into(),
    };

    save_run(&workspace, &run("Older Run!", "2026-01-01T00:00:00Z")).expect("saves");
    save_run(&workspace, &run("newer", "2026-02-01T00:00:00Z")).expect("saves");

    let listed = list_runs(&workspace);
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].run.name, "newer");
    assert_eq!(listed[1].file_name, "older-run.json");

    assert_eq!(
        load_run(&workspace, "Older Run!").map(|saved| saved.project_bumps),
        Some([("web".to_string(), VersionBump::Minor)].into())
    );
    delete_run(&workspace, "older-run").expect("deletes");
    assert!(load_run(&workspace, "Older Run!").is_none());
    assert!(delete_run(&workspace, "ghost").is_err());
}

#[test]
fn a_saved_run_without_libraries_is_listed_when_it_has_steps_that_need_none() {
    let root = TempDir::new("config");
    let workspace = Workspace::open(root.path()).expect("workspace");
    let run = |name: &str, steps: Vec<StepName>| SavedRun {
        name: name.to_string(),
        saved_at: "2026-01-01T00:00:00Z".to_string(),
        projects: vec!["web".to_string()],
        packages: vec![],
        steps,
        concurrency: 2,
        mode: ExecutionMode::PerProject,
        extra_install_args: vec![],
        version: VersionPolicy::default(),
        project_bumps: Default::default(),
    };

    save_run(
        &workspace,
        &run("bump", vec![StepName::Version, StepName::Audit]),
    )
    .expect("saves");
    save_run(&workspace, &run("install", vec![StepName::Install])).expect("saves");

    let names: Vec<String> = list_runs(&workspace)
        .into_iter()
        .map(|entry| entry.run.name)
        .collect();
    assert_eq!(names, vec!["bump".to_string()]);
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
fn a_saved_run_keeps_the_pack_choice_and_older_packages_load_unpacked() {
    let root = TempDir::new("config-pack");
    let workspace = Workspace::open(root.path()).expect("workspace");
    std::fs::create_dir_all(workspace.runs_directory()).unwrap();
    std::fs::write(
        workspace.runs_directory().join("packed.json"),
        r#"{
          "name": "packed",
          "savedAt": "2026-01-01T00:00:00Z",
          "projects": ["web"],
          "packages": [
            { "name": "@acme/core", "version": "1.2.3", "pack": "save" },
            { "name": "left-pad", "version": "1.3.0", "saveDev": false }
          ],
          "steps": ["install"],
          "concurrency": 2
        }"#,
    )
    .unwrap();

    let run = load_run(&workspace, "packed").expect("the run is listed");

    assert_eq!(run.packages[0].pack, Some(PackMode::Save));
    assert_eq!(run.packages[1].pack, None);
    let written = serde_json::to_value(&run.packages).unwrap();
    assert_eq!(written[0]["pack"], "save");
    assert!(written[1].get("pack").is_none(), "{written}");
}

#[test]
fn a_config_package_can_ask_for_a_peer_install() {
    let config = parse_config_text(
        r#"{"packages":[{ "name": "@acme/ui", "version": "9.0.0", "savePeer": true }]}"#,
    )
    .unwrap();
    assert!(config.packages[0].save_peer);
}

#[test]
fn a_saved_run_with_the_audit_report_step_loads() {
    let root = TempDir::new("config-audit-run");
    let workspace = Workspace::open(root.path()).expect("workspace");
    std::fs::create_dir_all(workspace.runs_directory()).unwrap();
    std::fs::write(
        workspace.runs_directory().join("audit.json"),
        r#"{
          "name": "audit",
          "savedAt": "2026-10-22T00:00:00Z",
          "projects": ["web"],
          "packages": [],
          "steps": ["audit-report", "build"],
          "concurrency": 2
        }"#,
    )
    .unwrap();

    let run = load_run(&workspace, "audit").expect("the run is listed");

    assert_eq!(run.steps, vec![StepName::AuditReport, StepName::Build]);
}

fn summary_json(steps: &str, project_extra: &str) -> String {
    format!(
        r#"{{"label":"run","packages":[],"projectCount":1,"concurrency":1,"steps":{steps},
        "mode":"per-project","dryRun":false,"startedAt":"2026-10-22T10:00:00.000Z",
        "finishedAt":"2026-10-22T10:01:00.000Z","totalDurationMs":60000,"busyDurationMs":60000,
        "logFile":null,"okCount":1,"warnCount":0,"failedCount":0,"skippedCount":0,"aborted":false,
        "projects":[{{"name":"web","directory":"C:/repos/web","status":"ok","durationMs":60000,
        "stepTimings":[],"error":"","warning":""{project_extra}}}]}}"#
    )
}

#[test]
fn run_summaries_load_with_and_without_audit_counts() {
    let root = TempDir::new("config-audit-summaries");
    let logs = root.mkdir("logs");
    std::fs::write(logs.join("old.json"), summary_json(r#"["install"]"#, "")).unwrap();
    std::fs::write(
        logs.join("new.json"),
        summary_json(
            r#"["audit-report"]"#,
            r#","audit":{"info":0,"low":1,"moderate":2,"high":3,"critical":4,"total":10}"#,
        ),
    )
    .unwrap();

    let summaries = list_summaries(&logs);

    let audits: Vec<Option<AuditCounts>> = summaries
        .iter()
        .map(|summary| summary.projects[0].audit)
        .collect();
    assert_eq!(audits.len(), 2, "{summaries:?}");
    assert!(audits.contains(&None));
    assert!(audits.contains(&Some(AuditCounts {
        info: 0,
        low: 1,
        moderate: 2,
        high: 3,
        critical: 4,
        total: 10,
    })));
}
