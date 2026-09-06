mod common;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use common::{manifest, TempDir};
use deptide_core::domain::{DetectedProject, ExecutionMode, Job, JobStatus, PackageSpec, StepName};
use deptide_core::execution::{
    diff_dependencies, execute_run, read_top_level_versions, restore_project, ProgressSink,
    RunContext, RunEvent, RunOptions,
};
use deptide_core::scan::{
    branch_suffix, collect_dependency_candidates, compile_branch_pattern, current_branch,
    read_project_at,
};
use deptide_core::util::version::is_newer;

struct Silent;

impl ProgressSink for Silent {
    fn emit(&self, _event: RunEvent) {}
}

struct AbortOneOnStart {
    context: Mutex<Option<Arc<RunContext>>>,
}

impl ProgressSink for AbortOneOnStart {
    fn emit(&self, event: RunEvent) {
        if let RunEvent::JobChanged { job, .. } = &event {
            if job.name == "web" && job.status == JobStatus::Running {
                if let Some(context) = self.context.lock().unwrap().as_ref() {
                    let _ = context.abort_job("api");
                }
            }
        }
    }
}

fn job(name: &str, directory: std::path::PathBuf, depends_on: Vec<&str>) -> Job {
    Job {
        name: name.to_string(),
        directory,
        packages: vec![PackageSpec::new("left-pad", "1.3.0")],
        steps: vec![StepName::Install, StepName::Build],
        install_args: vec![],
        audit_fix_args: vec![],
        depends_on: depends_on.into_iter().map(String::from).collect(),
        command: None,
    }
}

#[test]
fn branch_is_read_from_the_git_head_and_the_suffix_extracted() {
    let root = TempDir::new("git");
    root.write("lib/.git/HEAD", "ref: refs/heads/ABC-123-new-widgets\n");
    root.write(
        "lib/package.json",
        &manifest("@acme/core", "3.1.0-ABC-100", &[]),
    );
    root.write(
        "lib/nested/package.json",
        &manifest("@acme/nested", "1.0.0", &[]),
    );
    root.write("worktree/.git", "gitdir: ../lib/.git\n");
    root.write("detached/.git/HEAD", "0123456789abcdef\n");

    assert_eq!(
        current_branch(&root.join("lib")).as_deref(),
        Some("ABC-123-new-widgets")
    );
    assert_eq!(
        current_branch(&root.join("lib/nested")).as_deref(),
        Some("ABC-123-new-widgets")
    );
    assert_eq!(
        current_branch(&root.join("worktree")).as_deref(),
        Some("ABC-123-new-widgets")
    );
    assert!(current_branch(&root.join("detached")).is_none());

    let pattern = compile_branch_pattern(r"^([A-Za-z]+-\d+)");
    assert_eq!(
        branch_suffix("ABC-123-new-widgets", &pattern).as_deref(),
        Some("ABC-123")
    );
    assert!(branch_suffix("main", &pattern).is_none());

    let custom = compile_branch_pattern(r"feature/(.+)$");
    assert_eq!(
        branch_suffix("feature/ABC-1-x", &custom).as_deref(),
        Some("ABC-1-x")
    );

    let library = read_project_at(&root.join("lib"), root.path()).unwrap();
    let consumer = DetectedProject {
        directory: root.join("app").to_string_lossy().to_string(),
        relative_path: "app".to_string(),
        proposed_name: "app".to_string(),
        package_name: Some("app".to_string()),
        version: Some("1.0.0".to_string()),
        branch: None,
        kind: deptide_core::domain::ProjectKind::Application,
        dependencies: BTreeMap::from([("@acme/core".to_string(), "^3.0.0".to_string())]),
        dev_dependencies: BTreeMap::new(),
        has_build_script: false,
    };

    let candidates = collect_dependency_candidates(&[library, consumer.clone()], &[consumer], "");
    assert_eq!(
        candidates[0].local_branch.as_deref(),
        Some("ABC-123-new-widgets")
    );
    assert_eq!(candidates[0].branch_suffix.as_deref(), Some("ABC-123"));
}

#[test]
fn dependency_diff_reports_added_removed_and_changed_top_level_packages() {
    let root = TempDir::new("diff");
    root.write(
        "app/node_modules/.package-lock.json",
        r#"{"packages":{"node_modules/a":{"version":"1.0.0"},"node_modules/b":{"version":"2.0.0"},"node_modules/b/node_modules/c":{"version":"9.9.9"}}}"#,
    );

    let before = read_top_level_versions(&root.join("app"));
    assert_eq!(before.len(), 2, "nested packages are not top level");

    let after = BTreeMap::from([
        ("a".to_string(), "1.1.0".to_string()),
        ("d".to_string(), "0.1.0".to_string()),
    ]);
    let changes = diff_dependencies(&before, &after);
    let described: Vec<String> = changes
        .iter()
        .map(|change| format!("{} {:?}->{:?}", change.name, change.before, change.after))
        .collect();

    assert_eq!(
        described,
        vec![
            "a Some(\"1.0.0\")->Some(\"1.1.0\")",
            "b Some(\"2.0.0\")->None",
            "d None->Some(\"0.1.0\")"
        ]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn files_are_backed_up_before_the_first_step_and_can_be_restored() {
    let root = TempDir::new("backup");
    root.write(
        "app/package.json",
        &manifest("app", "1.0.0", &[("left-pad", "1.2.0")]),
    );
    root.write(
        "app/package-lock.json",
        "{\"name\":\"app\",\"lockfileVersion\":3}",
    );
    let backups = root.mkdir("backups");

    let context = RunContext::with_backups(
        "run".to_string(),
        "backup test".to_string(),
        vec![job("app", root.join("app"), vec![])],
        RunOptions {
            concurrency: 1,
            mode: ExecutionMode::PerProject,
            dry_run: true,
        },
        Arc::new(Silent),
        None,
        Some(backups.clone()),
    );
    execute_run(context.clone()).await;

    let snapshot = context.snapshot(true);
    assert!(snapshot.jobs[0].backup_available);
    assert!(snapshot.jobs[0]
        .log
        .iter()
        .any(|line| line.starts_with("backed up package.json")));

    std::fs::write(root.join("app/package.json"), "{\"broken\":true}").unwrap();
    std::fs::remove_file(root.join("app/package-lock.json")).unwrap();

    let restored = context.restore_job_files("app").unwrap();
    assert_eq!(restored, vec!["package.json", "package-lock.json"]);
    assert!(std::fs::read_to_string(root.join("app/package.json"))
        .unwrap()
        .contains("left-pad"));
    assert!(root.join("app/package-lock.json").exists());

    assert!(restore_project(&root.join("missing"), &root.join("app")).is_err());
    assert!(context.restore_job_files("ghost").is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_single_project_can_be_stopped_while_others_continue() {
    let root = TempDir::new("abort-one");
    root.write("web/package.json", &manifest("web", "1.0.0", &[]));
    root.write("api/package.json", &manifest("api", "1.0.0", &[]));

    let sink = Arc::new(AbortOneOnStart {
        context: Mutex::new(None),
    });
    let context = RunContext::new(
        "run".to_string(),
        "abort one".to_string(),
        vec![
            job("web", root.join("web"), vec![]),
            job("api", root.join("api"), vec!["web"]),
        ],
        RunOptions {
            concurrency: 2,
            mode: ExecutionMode::PerProject,
            dry_run: true,
        },
        sink.clone(),
        None,
    );
    *sink.context.lock().unwrap() = Some(context.clone());

    let snapshot = execute_run(context.clone()).await;

    assert_eq!(snapshot.jobs[0].status, JobStatus::Ok);
    assert_eq!(snapshot.jobs[1].status, JobStatus::Skipped);
    assert!(snapshot.jobs[1].error.contains("stopped"));
    assert!(
        !snapshot.aborted,
        "stopping one project does not abort the run"
    );
}

#[test]
fn version_comparison_understands_prereleases_and_prefixes() {
    assert!(is_newer("3.1.0", "3.0.9"));
    assert!(is_newer("v3.1.0", "3.0.9"));
    assert!(!is_newer("3.0.9-ABC-1", "3.0.9"));
    assert!(!is_newer("3.0.9", "3.0.9"));
    assert!(is_newer("3.10.0", "3.9.9"));
}
