mod common;

use std::path::Path;
use std::sync::Arc;

use common::{manifest, read_version, stacked_branches, GitRepo, Lines, TempDir};
use deptide_core::domain::{
    BumpWhen, ConfiguredProject, ExecutionMode, JobStatus, LookupCause, PackageSpec, PlanPreview,
    Reason, ReasonPreview, RunPlan, StepName, UpdateConfig, VersionBump, VersionPolicy,
    VersionPreview,
};
use deptide_core::execution::{
    build_jobs, execute_run, next_version, preview_plan, RunContext, RunOptions,
};
use deptide_core::workspace::{load_config, save_config, Workspace};

fn plan(names: &[&str], packages: Vec<PackageSpec>, steps: Vec<StepName>) -> RunPlan {
    RunPlan {
        project_names: names.iter().map(|name| name.to_string()).collect(),
        packages,
        steps,
        mode: ExecutionMode::PerProject,
        concurrency: 2,
        dry_run: false,
        extra_install_args: vec!["--force".to_string()],
        label: "preview".to_string(),
        save_as: None,
        version: VersionPolicy::default(),
        project_bumps: Default::default(),
    }
}

/// web declares @acme/core in dependencies and has a build script, api in devDependencies
/// without one, and api has its own install arguments.
fn workspace_with_projects() -> (TempDir, Workspace) {
    let root = TempDir::new("preview");
    let tool = root.mkdir("tool");
    root.write(
        "repos/web/package.json",
        &manifest("web", "1.4.2", &[("@acme/core", "^3.0.0")]),
    );
    root.write(
        "repos/api/package.json",
        r#"{"name":"api","version":"2.0.0","devDependencies":{"@acme/core":"3.0.0"}}"#,
    );
    root.write(
        "repos/other/package.json",
        r#"{"name":"other","version":"1.0.0","dependencies":{"left-pad":"1.0.0"}}"#,
    );

    let workspace = Workspace::open(&tool).expect("workspace");
    let mut api = ConfiguredProject::new("api", "../repos/api");
    api.install_args = Some(vec![r"--cache=C:\npm cache".to_string()]);
    let config = UpdateConfig {
        projects: vec![
            ConfiguredProject::new("web", "../repos/web"),
            api,
            ConfiguredProject::new("other", "../repos/other"),
        ],
        install_args: vec!["--no-fund".to_string()],
        ..UpdateConfig::default()
    };
    save_config(&workspace, &config).expect("config saved");
    (root, workspace)
}

fn commands(preview: &PlanPreview, job: &str) -> Vec<(StepName, Vec<String>, Vec<String>)> {
    preview
        .jobs
        .iter()
        .find(|entry| entry.name == job)
        .unwrap_or_else(|| panic!("no job {job} in {preview:?}"))
        .steps
        .iter()
        .map(|step| {
            let notes = step.notes.iter().map(|note| note.text.clone()).collect();
            (step.step, step.commands.clone(), notes)
        })
        .collect()
}

fn strings(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|line| line.to_string()).collect()
}

#[test]
fn the_next_version_follows_npm_version() {
    let cases = [
        ("1.4.2", VersionBump::Patch, Some("1.4.3")),
        ("1.4.2", VersionBump::Minor, Some("1.5.0")),
        ("1.4.2", VersionBump::Major, Some("2.0.0")),
        ("1.4.2-beta.1", VersionBump::Patch, Some("1.4.2")),
        ("1.4.2-beta.1", VersionBump::Minor, Some("1.5.0")),
        ("1.5.0-rc.0", VersionBump::Minor, Some("1.5.0")),
        ("1.5.0-rc.0", VersionBump::Major, Some("2.0.0")),
        ("2.0.0-alpha", VersionBump::Major, Some("2.0.0")),
        ("1.0.0+build.5", VersionBump::Patch, Some("1.0.1")),
        ("v1.2.3", VersionBump::Minor, Some("1.3.0")),
        ("1.2", VersionBump::Patch, None),
        ("", VersionBump::Patch, None),
    ];

    for (current, bump, expected) in cases {
        assert_eq!(
            next_version(current, bump).as_deref(),
            expected,
            "{current} {bump:?}"
        );
    }
}

#[test]
fn the_preview_lists_every_step_with_the_npm_lines_it_runs() {
    let (_root, workspace) = workspace_with_projects();
    let config = load_config(&workspace).unwrap();
    let plan = plan(
        &["web", "api", "other", "ghost"],
        vec![PackageSpec::new("@acme/core", "^3.1.0")],
        vec![
            StepName::Uninstall,
            StepName::Install,
            StepName::Audit,
            StepName::AuditReport,
            StepName::Build,
        ],
    );

    let preview = preview_plan(&workspace, &config, &plan);

    assert_eq!(
        commands(&preview, "web"),
        vec![
            (
                StepName::Uninstall,
                strings(&["npm uninstall @acme/core"]),
                vec![]
            ),
            (
                StepName::Install,
                strings(&[r#"npm install "@acme/core@^3.1.0" --no-fund --force"#]),
                vec![]
            ),
            (StepName::Audit, strings(&["npm audit fix"]), vec![]),
            (
                StepName::AuditReport,
                strings(&["npm audit --json"]),
                vec![]
            ),
            (StepName::Build, strings(&["npm run build"]), vec![]),
        ]
    );
    assert_eq!(
        commands(&preview, "api"),
        vec![
            (
                StepName::Uninstall,
                strings(&["npm uninstall @acme/core"]),
                vec![]
            ),
            (
                StepName::Install,
                strings(&[
                    r#"npm install "@acme/core@^3.1.0" --save-dev "--cache=C:\npm cache" --force"#
                ]),
                vec![]
            ),
            (StepName::Audit, strings(&["npm audit fix"]), vec![]),
            (
                StepName::AuditReport,
                strings(&["npm audit --json"]),
                vec![]
            ),
            (
                StepName::Build,
                vec![],
                strings(&["no build script, step skipped"])
            ),
        ]
    );
    assert_eq!(preview.missing, strings(&["ghost"]));
    assert_eq!(preview.without_packages, strings(&["other"]));
    assert_eq!(preview.nothing_to_do, None);
}

/// A workspace beside `project_parent` whose only project, web, is its `repo` folder.
fn workspace_for_repo(project_parent: &Path) -> Workspace {
    let tool = project_parent.join("tool");
    std::fs::create_dir_all(&tool).unwrap();
    let workspace = Workspace::open(&tool).expect("workspace");
    let config = UpdateConfig {
        projects: vec![ConfiguredProject::new("web", "../repo")],
        ..UpdateConfig::default()
    };
    save_config(&workspace, &config).expect("config saved");
    workspace
}

fn version_plan(when: BumpWhen, own_level: Option<VersionBump>) -> RunPlan {
    let mut plan = plan(&["web"], vec![], vec![StepName::Version]);
    plan.version = VersionPolicy {
        bump: VersionBump::Minor,
        when,
    };
    if let Some(level) = own_level {
        plan.project_bumps.insert("web".to_string(), level);
    }
    plan
}

/// The preview of the version step, then what a real run of the same plan did to the version.
async fn preview_then_run(
    project_parent: &Path,
    when: BumpWhen,
    own_level: Option<VersionBump>,
) -> (VersionPreview, String, Vec<String>) {
    let workspace = workspace_for_repo(project_parent);
    let config = load_config(&workspace).unwrap();
    let plan = version_plan(when, own_level);

    let preview = preview_plan(&workspace, &config, &plan);
    let version = preview.jobs[0]
        .version
        .clone()
        .unwrap_or_else(|| panic!("no version preview in {preview:?}"));

    let sink = Arc::new(Lines::default());
    let context = RunContext::new(
        "preview-run".to_string(),
        "preview".to_string(),
        build_jobs(&workspace, &config, &plan).jobs,
        RunOptions {
            concurrency: 1,
            mode: ExecutionMode::PerProject,
            dry_run: false,
        },
        sink.clone(),
        None,
    );
    let snapshot = execute_run(context).await;
    let lines = sink.seen.lock().unwrap().clone();
    assert_eq!(snapshot.jobs[0].status, JobStatus::Ok, "{lines:?}");

    (version, read_version(&project_parent.join("repo")), lines)
}

fn because(reason: Reason, text: &str) -> ReasonPreview {
    ReasonPreview {
        reason,
        text: text.to_string(),
    }
}

fn assert_run_matches(preview: &VersionPreview, version_after: &str, lines: &[String]) {
    match preview {
        VersionPreview::Bump { next, reason, .. } => {
            assert_eq!(version_after, next, "{lines:?}");
            if let Some(reason) = reason {
                assert!(
                    lines.contains(&format!("{}, bumping", reason.text)),
                    "{reason:?} not in {lines:?}"
                );
            }
        }
        VersionPreview::Skip {
            current, reason, ..
        } => {
            assert_eq!(Some(version_after), current.as_deref(), "{lines:?}");
            assert!(
                lines.contains(&format!("{}, bump skipped", reason.text)),
                "{reason:?} not in {lines:?}"
            );
        }
        VersionPreview::Fail { .. } => panic!("no case here fails: {preview:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn always_previews_the_bump_with_the_project_own_level() {
    let root = TempDir::new("preview-always");
    root.write("repo/package.json", r#"{"name":"web","version":"1.4.2"}"#);

    let (preview, after, lines) =
        preview_then_run(root.path(), BumpWhen::Always, Some(VersionBump::Major)).await;

    assert_eq!(
        preview,
        VersionPreview::Bump {
            current: Some("1.4.2".to_string()),
            next: "2.0.0".to_string(),
            level: VersionBump::Major,
            reason: None,
        }
    );
    assert_run_matches(&preview, &after, &lines);
}

#[tokio::test(flavor = "multi_thread")]
async fn same_as_main_previews_a_bump_while_equal_and_a_skip_once_different() {
    let equal = GitRepo::new("preview-main-equal");
    equal.commit_version("", "1.0.0");
    equal.git(&["checkout", "-q", "-b", "A"]);
    equal.commit_other("readme");

    let (preview, after, lines) =
        preview_then_run(equal.outside(), BumpWhen::SameAsMain, None).await;

    assert_eq!(
        preview,
        VersionPreview::Bump {
            current: Some("1.0.0".to_string()),
            next: "1.1.0".to_string(),
            level: VersionBump::Minor,
            reason: Some(because(
                Reason::StillEqualsMain {
                    version: Some("1.0.0".to_string()),
                },
                "version 1.0.0 still equals main",
            )),
        }
    );
    assert_run_matches(&preview, &after, &lines);

    let differs = stacked_branches("preview-main-differs");
    let (preview, after, lines) =
        preview_then_run(differs.outside(), BumpWhen::SameAsMain, None).await;

    assert_eq!(
        preview,
        VersionPreview::Skip {
            current: Some("1.1.0".to_string()),
            level: VersionBump::Minor,
            reason: because(
                Reason::DiffersFromMain {
                    version: Some("1.1.0".to_string()),
                    main: "1.0.0".to_string(),
                },
                "version 1.1.0 already differs from main (1.0.0)",
            ),
        }
    );
    assert_run_matches(&preview, &after, &lines);
}

#[tokio::test(flavor = "multi_thread")]
async fn not_bumped_on_branch_previews_the_base_or_the_commit_that_bumped() {
    let unchanged = stacked_branches("preview-branch-unchanged");

    let (preview, after, lines) = preview_then_run(
        unchanged.outside(),
        BumpWhen::NotBumpedOnBranch,
        Some(VersionBump::Patch),
    )
    .await;

    assert_eq!(
        preview,
        VersionPreview::Bump {
            current: Some("1.1.0".to_string()),
            next: "1.1.1".to_string(),
            level: VersionBump::Patch,
            reason: Some(because(
                Reason::NotBumpedOnBranch {
                    version: Some("1.1.0".to_string()),
                    base: "A".to_string(),
                    commit: unchanged.short("A"),
                },
                &format!(
                    "no commit of this branch changed version 1.1.0 since A ({})",
                    unchanged.short("A")
                ),
            )),
        }
    );
    assert_run_matches(&preview, &after, &lines);

    let bumped = stacked_branches("preview-branch-bumped");
    bumped.commit_version("", "1.2.0");
    let (preview, after, lines) =
        preview_then_run(bumped.outside(), BumpWhen::NotBumpedOnBranch, None).await;

    assert_eq!(
        preview,
        VersionPreview::Skip {
            current: Some("1.2.0".to_string()),
            level: VersionBump::Minor,
            reason: because(
                Reason::ChangedOnBranch {
                    version: Some("1.2.0".to_string()),
                    from: "1.1.0".to_string(),
                    to: "1.2.0".to_string(),
                    commit: bumped.short("HEAD"),
                },
                &format!(
                    "version 1.2.0 already changed on this branch (1.1.0 -> 1.2.0 in {})",
                    bumped.short("HEAD")
                ),
            ),
        }
    );
    assert_run_matches(&preview, &after, &lines);

    let outside_git = TempDir::new("preview-branch-no-git");
    outside_git.write("repo/package.json", r#"{"name":"web","version":"1.0.0"}"#);
    let (preview, after, lines) =
        preview_then_run(outside_git.path(), BumpWhen::NotBumpedOnBranch, None).await;

    assert_eq!(
        preview,
        VersionPreview::Skip {
            current: Some("1.0.0".to_string()),
            level: VersionBump::Minor,
            reason: because(
                Reason::BranchUnknown {
                    cause: LookupCause::NotARepository,
                },
                "cannot tell whether this branch bumped the version: the folder is not a git repository or git is not installed",
            ),
        }
    );
    assert_run_matches(&preview, &after, &lines);
}

/// The preview of the version step for a project whose package.json is `manifest`, then the
/// status of a real run of the same plan and the `version` field it left.
async fn preview_and_run_version(
    manifest: &str,
    level: VersionBump,
) -> (VersionPreview, JobStatus, serde_json::Value) {
    let root = TempDir::new("preview-version-field");
    root.write("repo/package.json", manifest);
    let workspace = workspace_for_repo(root.path());
    let config = load_config(&workspace).unwrap();
    let mut plan = version_plan(BumpWhen::Always, None);
    plan.version.bump = level;

    let preview = preview_plan(&workspace, &config, &plan);
    let version = preview.jobs[0]
        .version
        .clone()
        .unwrap_or_else(|| panic!("no version preview in {preview:?}"));

    let context = RunContext::new(
        "preview-run".to_string(),
        "preview".to_string(),
        build_jobs(&workspace, &config, &plan).jobs,
        RunOptions {
            concurrency: 1,
            mode: ExecutionMode::PerProject,
            dry_run: false,
        },
        Arc::new(Lines::default()),
        None,
    );
    let snapshot = execute_run(context).await;
    let text = std::fs::read_to_string(root.join("repo/package.json")).unwrap();
    let written: serde_json::Value = serde_json::from_str(&text).unwrap();

    (version, snapshot.jobs[0].status, written["version"].clone())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_version_previews_the_version_npm_starts_from_zero() {
    let manifests = [
        r#"{"name":"web"}"#,
        r#"{"name":"web","version":""}"#,
        r#"{"name":"web","version":null}"#,
    ];
    let expected = [
        (VersionBump::Patch, "0.0.1"),
        (VersionBump::Minor, "0.1.0"),
        (VersionBump::Major, "1.0.0"),
    ];

    for manifest in manifests {
        for (level, next_expected) in expected {
            let (preview, status, written) = preview_and_run_version(manifest, level).await;

            assert_eq!(status, JobStatus::Ok, "{manifest} {level:?}");
            assert_eq!(written, next_expected, "{manifest} {level:?}");
            assert!(
                matches!(&preview, VersionPreview::Bump { next, .. } if next == next_expected),
                "{manifest} {level:?}: preview {preview:?}, npm wrote {written}"
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_preview_predicts_what_npm_does_with_unusual_versions() {
    let versions = [
        r#""01.2.3""#,
        r#""1.2.3-!""#,
        r#""1.2.3+""#,
        r#""1.2.3-01""#,
        r#""1.2.3-a..b""#,
        r#""1.2""#,
        "1",
        r#""9007199254740992.0.0""#,
        r#""18446744073709551615.18446744073709551615.18446744073709551615""#,
        r#"" =v1.2.3 ""#,
        r#""v 1.2.3""#,
        r#""\ufeff1.2.3""#,
        r#""1.2.3-rc.1+b""#,
        r#""0.0.0-x""#,
        r#""9007199254740991.9007199254740991.9007199254740991""#,
    ];

    for version in versions {
        for level in [VersionBump::Patch, VersionBump::Minor, VersionBump::Major] {
            let manifest = format!(r#"{{"name":"web","version":{version}}}"#);
            let before: serde_json::Value = serde_json::from_str(&manifest)
                .map(|m: serde_json::Value| m["version"].clone())
                .unwrap();
            let (preview, status, written) = preview_and_run_version(&manifest, level).await;

            match status {
                JobStatus::Ok => assert!(
                    matches!(&preview, VersionPreview::Bump { next, .. } if written == next.as_str()),
                    "{version} {level:?}: npm wrote {written}, preview {preview:?}"
                ),
                _ => {
                    assert_eq!(written, before, "{version} {level:?}");
                    assert!(
                        matches!(preview, VersionPreview::Fail { .. }),
                        "{version} {level:?}: npm failed, preview {preview:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn numbers_npm_rejects_have_no_next_version() {
    let largest = "18446744073709551615.18446744073709551615.18446744073709551615";
    for bump in [VersionBump::Patch, VersionBump::Minor, VersionBump::Major] {
        let next = std::panic::catch_unwind(|| next_version(largest, bump));
        assert!(matches!(next, Ok(None)), "{bump:?}: {next:?}");
    }
}

#[test]
fn the_preview_gives_each_reason_as_a_kind_with_its_values_beside_the_english_text() {
    let differs = stacked_branches("preview-reason-kinds");
    let workspace = workspace_for_repo(differs.outside());
    let config = load_config(&workspace).unwrap();
    let mut plan = version_plan(BumpWhen::SameAsMain, None);
    plan.steps.push(StepName::Build);

    let json = serde_json::to_value(preview_plan(&workspace, &config, &plan)).unwrap();

    assert_eq!(
        json["jobs"][0]["version"]["reason"],
        serde_json::json!({
            "kind": "differsFromMain",
            "params": { "version": "1.1.0", "main": "1.0.0" },
            "text": "version 1.1.0 already differs from main (1.0.0)",
        })
    );
    assert_eq!(
        json["jobs"][0]["steps"][1]["notes"],
        serde_json::json!([{
            "kind": "noBuildScript",
            "params": {},
            "text": "no build script, step skipped",
        }])
    );

    plan.steps = vec![StepName::Install];
    let json = serde_json::to_value(preview_plan(&workspace, &config, &plan)).unwrap();
    assert_eq!(
        json["nothingToDo"],
        serde_json::json!({
            "kind": "noLibraries",
            "params": {},
            "text": "Nothing to do: without libraries only the version, audit, audit report and build steps can run",
        })
    );
}

#[test]
fn an_unavailable_branch_lookup_gives_its_cause_as_a_code_beside_the_english_text() {
    let outside_git = TempDir::new("preview-lookup-cause");
    outside_git.write("repo/package.json", r#"{"name":"web","version":"1.0.0"}"#);
    let workspace = workspace_for_repo(outside_git.path());
    let config = load_config(&workspace).unwrap();
    let plan = version_plan(BumpWhen::NotBumpedOnBranch, None);

    let json = serde_json::to_value(preview_plan(&workspace, &config, &plan)).unwrap();

    assert_eq!(
        json["jobs"][0]["version"]["reason"],
        serde_json::json!({
            "kind": "branchUnknown",
            "params": { "cause": "notARepository" },
            "text": "cannot tell whether this branch bumped the version: the folder is not a git repository or git is not installed",
        })
    );
}
