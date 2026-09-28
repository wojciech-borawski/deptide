mod common;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use common::TempDir;
use deptide_core::domain::{
    ConfiguredProject, ExecutionMode, Job, JobSnapshot, JobStatus, PackageSpec, RunPlan, StepName,
    UpdateConfig, VersionPolicy,
};
use deptide_core::execution::{
    build_jobs, execute_run, ProgressSink, RunContext, RunEvent, RunOptions,
};
use deptide_core::workspace::{save_config, Workspace};

struct AbortOnFirstLine {
    context: Mutex<Weak<RunContext>>,
    seen: Mutex<Vec<String>>,
}

impl ProgressSink for AbortOnFirstLine {
    fn emit(&self, event: RunEvent) {
        if let RunEvent::LogLine { line, .. } = &event {
            self.seen.lock().unwrap().push(line.clone());
            if !line.starts_with("$ ") {
                if let Some(context) = self.context.lock().unwrap().upgrade() {
                    context.abort();
                }
            }
        }
    }
}

#[derive(Default)]
struct Collect {
    lines: Mutex<Vec<String>>,
}

impl ProgressSink for Collect {
    fn emit(&self, event: RunEvent) {
        if let RunEvent::LogLine { line, .. } = event {
            self.lines.lock().unwrap().push(line);
        }
    }
}

fn workspace() -> (TempDir, Workspace) {
    workspace_with_manifest(
        r#"{"name":"npm-real-app","version":"1.0.0","private":true,"dependencies":{"left-pad":"1.2.0"}}"#,
    )
}

fn workspace_with_manifest(manifest: &str) -> (TempDir, Workspace) {
    let root = TempDir::new("npm-real");
    let tool = root.mkdir("tool");
    root.write("repos/app/package.json", manifest);

    let workspace = Workspace::open(&tool).expect("workspace");
    let config = UpdateConfig {
        projects: vec![ConfiguredProject::new("app", "../repos/app")],
        install_args: vec!["--no-fund".to_string(), "--no-audit".to_string()],
        ..UpdateConfig::default()
    };
    save_config(&workspace, &config).expect("saved");
    (root, workspace)
}

fn plan(steps: Vec<StepName>) -> RunPlan {
    RunPlan {
        project_names: vec!["app".to_string()],
        packages: vec![PackageSpec::new("left-pad", "1.3.0")],
        steps,
        mode: ExecutionMode::PerProject,
        concurrency: 1,
        dry_run: false,
        extra_install_args: vec![],
        label: "npm real".to_string(),
        save_as: None,
        version: VersionPolicy::default(),
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "spawns real npm and needs network access"]
async fn real_uninstall_and_install_replace_the_version() {
    let (root, workspace) = workspace();
    let config = deptide_core::workspace::load_config(&workspace).unwrap();
    let jobs = build_jobs(
        &workspace,
        &config,
        &plan(vec![StepName::Uninstall, StepName::Install]),
    )
    .jobs;

    let sink = Arc::new(Collect::default());
    let context = RunContext::new(
        "npm-1".to_string(),
        "npm real".to_string(),
        jobs,
        RunOptions {
            concurrency: 1,
            mode: ExecutionMode::PerProject,
            dry_run: false,
        },
        sink.clone(),
        None,
    );

    let snapshot = execute_run(context).await;

    assert_eq!(
        snapshot.jobs[0].status,
        JobStatus::Ok,
        "log: {:?}",
        sink.lines.lock().unwrap()
    );
    assert_eq!(snapshot.jobs[0].step_timings.len(), 2);
    assert!(snapshot.summary.unwrap().total_duration_ms > 0);

    let verified = &snapshot.jobs[0].installed;
    assert_eq!(verified.len(), 1);
    assert!(verified[0].matches, "verification: {verified:?}");
    assert_eq!(verified[0].installed.as_deref(), Some("1.3.0"));
    assert!(verified[0]
        .integrity
        .as_deref()
        .is_some_and(|hash| hash.starts_with("sha")));
    assert!(sink
        .lines
        .lock()
        .unwrap()
        .iter()
        .any(|line| line.starts_with("verified left-pad@1.3.0")));

    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("repos/app/package.json")).unwrap(),
    )
    .unwrap();
    let range = manifest["dependencies"]["left-pad"]
        .as_str()
        .unwrap_or_default();
    assert!(
        range.trim_start_matches('^') == "1.3.0",
        "manifest range: {range}"
    );
    assert!(root
        .join("repos/app/node_modules/left-pad/package.json")
        .exists());
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "spawns real npm and needs network access"]
async fn aborting_kills_the_running_npm_process() {
    let (_root, workspace) = workspace();
    let config = deptide_core::workspace::load_config(&workspace).unwrap();
    let jobs = build_jobs(
        &workspace,
        &config,
        &plan(vec![StepName::Install, StepName::Build]),
    )
    .jobs;

    let sink = Arc::new(AbortOnFirstLine {
        context: Mutex::new(Weak::new()),
        seen: Mutex::new(Vec::new()),
    });
    let context = RunContext::new(
        "npm-2".to_string(),
        "npm real".to_string(),
        jobs,
        RunOptions {
            concurrency: 1,
            mode: ExecutionMode::PerProject,
            dry_run: false,
        },
        sink.clone(),
        None,
    );
    *sink.context.lock().unwrap() = Arc::downgrade(&context);

    let started = std::time::Instant::now();
    let snapshot = tokio::time::timeout(std::time::Duration::from_secs(60), execute_run(context))
        .await
        .expect("an aborted run finishes promptly");

    assert!(snapshot.aborted);
    assert_eq!(snapshot.jobs[0].status, JobStatus::Skipped);
    assert!(started.elapsed().as_secs() < 60);
    assert!(!sink.seen.lock().unwrap().is_empty());
}

async fn run_uninstall_and_install(manifest: &str) -> (serde_json::Value, Vec<String>) {
    let (root, workspace) = workspace_with_manifest(manifest);
    let config = deptide_core::workspace::load_config(&workspace).unwrap();
    let jobs = build_jobs(
        &workspace,
        &config,
        &plan(vec![StepName::Uninstall, StepName::Install]),
    )
    .jobs;

    let sink = Arc::new(Collect::default());
    let context = RunContext::new(
        "npm-peer".to_string(),
        "npm real".to_string(),
        jobs,
        RunOptions {
            concurrency: 1,
            mode: ExecutionMode::PerProject,
            dry_run: false,
        },
        sink.clone(),
        None,
    );

    let snapshot = execute_run(context).await;
    let lines = sink.lines.lock().unwrap().clone();
    assert_eq!(snapshot.jobs[0].status, JobStatus::Ok, "log: {lines:?}");

    let manifest = serde_json::from_str(
        &std::fs::read_to_string(root.join("repos/app/package.json")).unwrap(),
    )
    .unwrap();
    (manifest, lines)
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "spawns real npm and needs network access"]
async fn a_peer_range_survives_updating_a_peer_that_is_also_a_dev_dependency() {
    let (manifest, lines) = run_uninstall_and_install(
        r#"{"name":"npm-real-lib","version":"1.0.0","private":true,
            "peerDependencies":{"left-pad":"^1.0.0"},"devDependencies":{"left-pad":"^1.2.0"}}"#,
    )
    .await;

    assert_eq!(
        manifest["peerDependencies"]["left-pad"], "^1.0.0",
        "manifest: {manifest}"
    );
    assert_eq!(
        manifest["devDependencies"]["left-pad"], "^1.3.0",
        "manifest: {manifest}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line == "restored peerDependencies.left-pad = ^1.0.0"),
        "log: {lines:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "spawns real npm and needs network access"]
async fn a_dependency_that_is_also_a_peer_keeps_its_new_range_and_its_peer_range() {
    let (manifest, lines) = run_uninstall_and_install(
        r#"{"name":"npm-real-lib","version":"1.0.0","private":true,
            "dependencies":{"left-pad":"^1.2.0"},"peerDependencies":{"left-pad":"^1.0.0"}}"#,
    )
    .await;

    assert_eq!(
        manifest["dependencies"]["left-pad"], "^1.3.0",
        "manifest: {manifest}"
    );
    assert_eq!(
        manifest["peerDependencies"]["left-pad"], "^1.0.0",
        "manifest: {manifest}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line == "restored peerDependencies.left-pad = ^1.0.0"),
        "log: {lines:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "spawns real npm and needs network access"]
async fn a_peer_only_package_gets_a_caret_range_for_the_new_version() {
    let (manifest, _lines) = run_uninstall_and_install(
        r#"{"name":"npm-real-lib","version":"1.0.0","private":true,
            "peerDependencies":{"left-pad":"^1.0.0"}}"#,
    )
    .await;

    assert_eq!(
        manifest["peerDependencies"]["left-pad"], "^1.3.0",
        "manifest: {manifest}"
    );
    assert!(
        manifest.get("devDependencies").is_none()
            || manifest["devDependencies"].get("left-pad").is_none(),
        "manifest: {manifest}"
    );
}

const OFFLINE_NPMRC: &str =
    "offline=true\nregistry=http://127.0.0.1:9/\naudit=false\nfund=false\nupdate-notifier=false\n";

const DEPENDENCY_AND_PEER: &str = r#"{"name":"npm-offline-lib","version":"1.0.0","private":true,
    "dependencies":{"left-pad":"^1.0.0"},"peerDependencies":{"left-pad":"^1.0.0"}}"#;

const DEV_AND_PEER: &str = r#"{"name":"npm-offline-lib","version":"1.0.0","private":true,
    "devDependencies":{"left-pad":"^1.2.0"},"peerDependencies":{"left-pad":"^1.0.0"}}"#;

const PEER_ONLY: &str = r#"{"name":"npm-offline-lib","version":"1.0.0","private":true,
    "peerDependencies":{"left-pad":"^1.0.0"}}"#;

const DEPENDENCY_NEWER_THAN_PEER: &str = r#"{"name":"npm-offline-lib","version":"1.0.0","private":true,
    "dependencies":{"left-pad":"^1.2.0"},"peerDependencies":{"left-pad":"^1.0.0"}}"#;

const BROKEN_MANIFEST: &str = "{ broken";

const UNWRITABLE: &str = "could not restore dependencies.left-pad = ^1.2.0, peerDependencies.left-pad = ^1.0.0: package.json is not valid JSON";

const SECTIONS: [&str; 3] = ["dependencies", "devDependencies", "peerDependencies"];

#[derive(Clone, Copy)]
enum StopAt {
    Never,
    Phase(StepName),
    LineStartingWith(&'static str),
}

impl StopAt {
    fn matches(&self, event: &RunEvent) -> bool {
        match (self, event) {
            (StopAt::Phase(step), RunEvent::PhaseChanged { phase, .. }) => *phase == Some(*step),
            (StopAt::LineStartingWith(prefix), RunEvent::LogLine { line, .. }) => {
                line.starts_with(prefix)
            }
            _ => false,
        }
    }
}

struct StopAndCollect {
    context: Mutex<Weak<RunContext>>,
    stop_at: StopAt,
    break_manifest_at: StopAt,
    manifest: PathBuf,
    lines: Mutex<Vec<String>>,
}

impl ProgressSink for StopAndCollect {
    fn emit(&self, event: RunEvent) {
        let stop = self.stop_at.matches(&event);
        if self.break_manifest_at.matches(&event) {
            std::fs::write(&self.manifest, BROKEN_MANIFEST).unwrap();
        }
        if let RunEvent::LogLine { line, .. } = &event {
            self.lines.lock().unwrap().push(line.clone());
        }
        if stop {
            if let Some(context) = self.context.lock().unwrap().upgrade() {
                context.abort_job("app").unwrap();
            }
        }
    }
}

struct OfflineRun {
    job: JobSnapshot,
    lines: Vec<String>,
    before: serde_json::Value,
    after: serde_json::Value,
    after_text: String,
}

impl OfflineRun {
    fn sections(manifest: &serde_json::Value) -> Vec<serde_json::Value> {
        SECTIONS
            .iter()
            .map(|field| manifest.get(field).cloned().unwrap_or_default())
            .collect()
    }

    fn assert_manifest_unchanged(&self) {
        assert_eq!(
            Self::sections(&self.after),
            Self::sections(&self.before),
            "manifest: {}\nlog: {:?}",
            self.after,
            self.lines
        );
    }

    fn has_line(&self, expected: &str) -> bool {
        self.lines.iter().any(|line| line == expected)
    }
}

/// Runs the plan with real npm in a project whose `.npmrc` blocks the network, so installs fail.
async fn run_offline(
    manifest: &str,
    steps: Vec<StepName>,
    mode: ExecutionMode,
    stop_at: StopAt,
) -> OfflineRun {
    run_offline_breaking_manifest(manifest, steps, mode, stop_at, StopAt::Never).await
}

async fn run_offline_breaking_manifest(
    manifest: &str,
    steps: Vec<StepName>,
    mode: ExecutionMode,
    stop_at: StopAt,
    break_manifest_at: StopAt,
) -> OfflineRun {
    let (root, workspace) = workspace_with_manifest(manifest);
    root.write("repos/app/.npmrc", OFFLINE_NPMRC);
    let config = deptide_core::workspace::load_config(&workspace).unwrap();
    let jobs = build_jobs(&workspace, &config, &plan(steps)).jobs;

    let sink = Arc::new(StopAndCollect {
        context: Mutex::new(Weak::new()),
        stop_at,
        break_manifest_at,
        manifest: root.join("repos/app/package.json"),
        lines: Mutex::new(Vec::new()),
    });
    let context = RunContext::new(
        "npm-offline".to_string(),
        "npm offline".to_string(),
        jobs,
        RunOptions {
            concurrency: 1,
            mode,
            dry_run: false,
        },
        sink.clone(),
        None,
    );
    *sink.context.lock().unwrap() = Arc::downgrade(&context);

    let mut snapshot = tokio::time::timeout(Duration::from_secs(180), execute_run(context))
        .await
        .expect("the offline run finishes");
    let after = std::fs::read_to_string(root.join("repos/app/package.json")).unwrap();
    let lines = sink.lines.lock().unwrap().clone();

    OfflineRun {
        job: snapshot.jobs.remove(0),
        lines,
        before: serde_json::from_str(manifest).unwrap(),
        after: serde_json::from_str(&after).unwrap_or_default(),
        after_text: after,
    }
}

async fn run_failing_install(manifest: &str) -> OfflineRun {
    let run = run_offline(
        manifest,
        vec![StepName::Uninstall, StepName::Install],
        ExecutionMode::PerProject,
        StopAt::Never,
    )
    .await;

    assert_eq!(run.job.status, JobStatus::Failed, "log: {:?}", run.lines);
    assert!(
        run.job.error.starts_with("npm install exited with code"),
        "error: {}",
        run.job.error
    );
    run
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_install_puts_back_a_package_in_dependencies_and_peer_dependencies() {
    let run = run_failing_install(DEPENDENCY_AND_PEER).await;

    run.assert_manifest_unchanged();
    assert!(run.has_line("restored dependencies.left-pad = ^1.0.0"));
    assert!(run.has_line("restored peerDependencies.left-pad = ^1.0.0"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_install_puts_back_a_package_in_dev_dependencies_and_peer_dependencies() {
    let run = run_failing_install(DEV_AND_PEER).await;

    run.assert_manifest_unchanged();
    assert!(run.has_line("restored devDependencies.left-pad = ^1.2.0"));
    assert!(run.has_line("restored peerDependencies.left-pad = ^1.0.0"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_peer_install_puts_back_the_peer_range() {
    let run = run_failing_install(PEER_ONLY).await;

    run.assert_manifest_unchanged();
    assert!(run.has_line("restored peerDependencies.left-pad = ^1.0.0"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_project_stopped_between_per_step_phases_gets_its_entries_back() {
    let run = run_offline(
        DEV_AND_PEER,
        vec![StepName::Uninstall, StepName::Install],
        ExecutionMode::PerStep,
        StopAt::Phase(StepName::Install),
    )
    .await;

    assert_eq!(run.job.status, JobStatus::Skipped, "log: {:?}", run.lines);
    assert_eq!(run.job.current_step, "");
    run.assert_manifest_unchanged();
    assert!(run.has_line("restored devDependencies.left-pad = ^1.2.0"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_project_stopped_before_its_install_gets_its_entries_back() {
    let run = run_offline(
        DEPENDENCY_AND_PEER,
        vec![StepName::Uninstall, StepName::Install],
        ExecutionMode::PerProject,
        StopAt::LineStartingWith("$ npm install"),
    )
    .await;

    assert_eq!(run.job.status, JobStatus::Skipped, "log: {:?}", run.lines);
    assert_eq!(run.job.error, "stopped before it finished");
    run.assert_manifest_unchanged();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_project_stopped_between_its_package_steps_gets_its_entries_back() {
    let run = run_offline(
        DEV_AND_PEER,
        vec![StepName::Uninstall, StepName::Version, StepName::Install],
        ExecutionMode::PerProject,
        StopAt::LineStartingWith("$ npm version"),
    )
    .await;

    assert_eq!(run.job.status, JobStatus::Skipped, "log: {:?}", run.lines);
    run.assert_manifest_unchanged();
    assert!(run.has_line("restored devDependencies.left-pad = ^1.2.0"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_install_puts_back_different_dependency_and_peer_ranges() {
    let run = run_failing_install(DEPENDENCY_NEWER_THAN_PEER).await;

    run.assert_manifest_unchanged();
    assert!(run.has_line("restored dependencies.left-pad = ^1.2.0"));
    assert!(run.has_line("restored peerDependencies.left-pad = ^1.0.0"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_restore_that_cannot_write_package_json_fails_the_project() {
    for (mode, at) in [
        (ExecutionMode::PerStep, StopAt::Phase(StepName::Install)),
        (
            ExecutionMode::PerProject,
            StopAt::LineStartingWith("$ npm install"),
        ),
    ] {
        let run = run_offline_breaking_manifest(
            DEPENDENCY_NEWER_THAN_PEER,
            vec![StepName::Uninstall, StepName::Install],
            mode,
            at,
            at,
        )
        .await;

        assert_eq!(
            run.job.status,
            JobStatus::Failed,
            "{mode:?} log: {:?}",
            run.lines
        );
        assert!(
            run.job.error.starts_with(UNWRITABLE),
            "{mode:?} error: {}",
            run.job.error
        );
        assert_eq!(run.job.current_step, "", "{mode:?}");
        assert_eq!(run.after_text, BROKEN_MANIFEST, "{mode:?}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn stopping_a_project_kills_its_running_command() {
    let root = TempDir::new("stop-command");
    let job = Job {
        name: "app".to_string(),
        directory: root.mkdir("app"),
        packages: vec![],
        steps: vec![],
        install_args: vec![],
        audit_fix_args: vec![],
        depends_on: vec![],
        command: Some(r#"node -e "console.log('ready'); setTimeout(() => {}, 60000)""#.to_string()),
        version: VersionPolicy::default(),
    };
    let sink = Arc::new(StopAndCollect {
        context: Mutex::new(Weak::new()),
        stop_at: StopAt::LineStartingWith("ready"),
        break_manifest_at: StopAt::Never,
        manifest: root.join("app/package.json"),
        lines: Mutex::new(Vec::new()),
    });
    let context = RunContext::new(
        "stop-command".to_string(),
        "stop command".to_string(),
        vec![job],
        RunOptions {
            concurrency: 1,
            mode: ExecutionMode::PerProject,
            dry_run: false,
        },
        sink.clone(),
        None,
    );
    *sink.context.lock().unwrap() = Arc::downgrade(&context);

    let snapshot = tokio::time::timeout(Duration::from_secs(30), execute_run(context))
        .await
        .expect("stopping the project kills the command");

    assert_eq!(snapshot.jobs[0].status, JobStatus::Skipped);
    assert!(sink
        .lines
        .lock()
        .unwrap()
        .iter()
        .any(|line| line == "ready"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_install_keeps_its_own_error_when_the_restore_fails_too() {
    let run = run_offline_breaking_manifest(
        DEPENDENCY_NEWER_THAN_PEER,
        vec![StepName::Uninstall, StepName::Install],
        ExecutionMode::PerProject,
        StopAt::Never,
        StopAt::LineStartingWith("$ npm install"),
    )
    .await;

    assert_eq!(run.job.status, JobStatus::Failed, "log: {:?}", run.lines);
    assert!(
        run.job.error.starts_with("npm install exited with code"),
        "error: {}",
        run.job.error
    );
    assert!(
        run.lines.iter().any(|line| line.starts_with(UNWRITABLE)),
        "log: {:?}",
        run.lines
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_uninstall_only_plan_leaves_the_removed_entries_removed() {
    let run = run_offline(
        DEV_AND_PEER,
        vec![StepName::Uninstall],
        ExecutionMode::PerProject,
        StopAt::Never,
    )
    .await;

    assert_eq!(run.job.status, JobStatus::Ok, "log: {:?}", run.lines);
    assert_eq!(
        OfflineRun::sections(&run.after),
        vec![serde_json::Value::Null; 3],
        "manifest: {}",
        run.after
    );
    assert!(
        !run.lines.iter().any(|line| line.contains("restore")),
        "log: {:?}",
        run.lines
    );
}
