mod common;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use common::TempDir;
use deptide_core::dependencies::{
    fetch_latest_versions, read_dependency_drift, DependencyRequest, LatestLookup,
};
use deptide_core::domain::{
    AuditCounts, ConfiguredProject, ExecutionMode, Job, JobSnapshot, JobStatus, PackageSpec,
    RunPlan, StepName, UpdateConfig, VersionBump, VersionPolicy,
};
use deptide_core::execution::{
    build_jobs, execute_run, preview_plan, ProgressSink, RunContext, RunEvent, RunOptions,
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
        project_bumps: Default::default(),
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
        packing: None,
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

const VULNERABLE_MANIFEST: &str =
    r#"{"name":"b","version":"1.0.0","dependencies":{"minimist":"1.2.0","lodash":"4.17.15"}}"#;

const BULK_ADVISORIES: &str = r#"{
    "lodash":[{"id":1,"url":"https://example.com/1","title":"Prototype Pollution","severity":"high","vulnerable_versions":"<4.17.21","cwe":[],"cvss":{"score":7.4,"vectorString":null}}],
    "minimist":[{"id":2,"url":"https://example.com/2","title":"Prototype Pollution","severity":"critical","vulnerable_versions":"<1.2.6","cwe":[],"cvss":{"score":9.8,"vectorString":null}}]
}"#;

/// Answers the registry's bulk advisory endpoint with `body` and every other request with 404.
fn serve_advisories(body: &'static str) -> u16 {
    use std::io::{BufRead, BufReader, Read, Write};

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream);
            let mut request_line = String::new();
            let _ = reader.read_line(&mut request_line);
            let mut length = 0;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap_or(0);
                }
            }
            let mut request_body = vec![0; length];
            let _ = reader.read_exact(&mut request_body);
            let (status, answer) = if request_line.contains("/-/npm/v1/security/advisories/bulk") {
                ("200 OK", body)
            } else {
                ("404 Not Found", "{}")
            };
            let response = format!(
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{answer}",
                answer.len()
            );
            let _ = reader.get_mut().write_all(response.as_bytes());
        }
    });
    port
}

struct AuditRun {
    job: JobSnapshot,
    lines: Vec<String>,
    files_before: Vec<Vec<u8>>,
    files_after: Vec<Vec<u8>>,
}

async fn run_audit_report(manifest: &str, lockfile: &str, npmrc: &str) -> AuditRun {
    let (root, workspace) = workspace_with_manifest(manifest);
    root.write("repos/app/package-lock.json", lockfile);
    root.write("repos/app/.npmrc", npmrc);
    let files = ["repos/app/package.json", "repos/app/package-lock.json"];
    let read_files = || -> Vec<Vec<u8>> {
        files
            .iter()
            .map(|file| std::fs::read(root.join(file)).unwrap())
            .collect()
    };
    let files_before = read_files();
    let config = deptide_core::workspace::load_config(&workspace).unwrap();
    let mut plan = plan(vec![StepName::AuditReport]);
    plan.packages.clear();
    let jobs = build_jobs(&workspace, &config, &plan).jobs;

    let sink = Arc::new(Collect::default());
    let context = RunContext::new(
        "npm-audit".to_string(),
        "npm audit".to_string(),
        jobs,
        RunOptions {
            concurrency: 1,
            mode: ExecutionMode::PerProject,
            dry_run: false,
        },
        sink.clone(),
        None,
    );
    let mut snapshot = tokio::time::timeout(Duration::from_secs(120), execute_run(context))
        .await
        .expect("the audit run finishes");
    let lines = sink.lines.lock().unwrap().clone();

    AuditRun {
        job: snapshot.jobs.remove(0),
        lines,
        files_before,
        files_after: read_files(),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_audit_report_counts_the_findings_per_severity_and_changes_no_file() {
    let port = serve_advisories(BULK_ADVISORIES);
    let npmrc =
        format!("registry=http://127.0.0.1:{port}/\nfetch-retries=0\nupdate-notifier=false\n");

    let run = run_audit_report(
        VULNERABLE_MANIFEST,
        include_str!("fixtures/audit/vulnerable-package-lock.json"),
        &npmrc,
    )
    .await;

    assert_eq!(run.job.status, JobStatus::Ok, "log: {:?}", run.lines);
    assert_eq!(
        run.job.audit,
        Some(AuditCounts {
            info: 0,
            low: 0,
            moderate: 0,
            high: 1,
            critical: 1,
            total: 2,
        }),
        "log: {:?}",
        run.lines
    );
    assert!(
        run.lines.contains(&"$ npm audit --json".to_string()),
        "log: {:?}",
        run.lines
    );
    assert!(
        run.lines.contains(
            &"audit: 2 vulnerabilities (1 critical, 1 high, 0 moderate, 0 low, 0 info)".to_string()
        ),
        "log: {:?}",
        run.lines
    );
    assert_eq!(run.files_after, run.files_before);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_audit_report_that_cannot_reach_the_registry_is_retried_and_fails() {
    let run = run_audit_report(
        VULNERABLE_MANIFEST,
        include_str!("fixtures/audit/vulnerable-package-lock.json"),
        "registry=http://127.0.0.1:9/\nfetch-retries=0\nupdate-notifier=false\n",
    )
    .await;

    assert_eq!(run.job.status, JobStatus::Failed, "log: {:?}", run.lines);
    assert_eq!(run.job.retries, 1, "log: {:?}", run.lines);
    assert!(
        run.job
            .error
            .starts_with("npm audit failed: request to http://127.0.0.1:9/"),
        "error: {}",
        run.job.error
    );
    assert_eq!(run.job.audit, None);
    assert_eq!(run.files_after, run.files_before);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_audit_report_without_a_lockfile_fails_with_npm_reason() {
    let (root, workspace) = workspace_with_manifest(VULNERABLE_MANIFEST);
    root.write("repos/app/.npmrc", OFFLINE_NPMRC);
    let config = deptide_core::workspace::load_config(&workspace).unwrap();
    let mut plan = plan(vec![StepName::AuditReport]);
    plan.packages.clear();
    let context = RunContext::new(
        "npm-audit".to_string(),
        "npm audit".to_string(),
        build_jobs(&workspace, &config, &plan).jobs,
        RunOptions {
            concurrency: 1,
            mode: ExecutionMode::PerProject,
            dry_run: false,
        },
        Arc::new(Collect::default()),
        None,
    );

    let snapshot = execute_run(context).await;

    assert_eq!(snapshot.jobs[0].status, JobStatus::Failed);
    assert_eq!(
        snapshot.jobs[0].error,
        "npm audit failed: ENOLOCK This command requires an existing lockfile."
    );
    assert!(!root.join("repos/app/package-lock.json").exists());
}

/// Runs `plan` for real and returns the npm lines the run logged and the ones the preview listed.
async fn logged_and_previewed_commands(
    manifest: &str,
    plan: RunPlan,
) -> (Vec<String>, Vec<String>) {
    let (root, workspace) = workspace_with_manifest(manifest);
    root.write("repos/app/.npmrc", OFFLINE_NPMRC);
    let config = deptide_core::workspace::load_config(&workspace).unwrap();

    let preview = preview_plan(&workspace, &config, &plan);
    let previewed: Vec<String> = preview
        .jobs
        .iter()
        .flat_map(|job| job.steps.iter().flat_map(|step| step.commands.clone()))
        .collect();

    let sink = Arc::new(Collect::default());
    let context = RunContext::new(
        "npm-preview".to_string(),
        "npm preview".to_string(),
        build_jobs(&workspace, &config, &plan).jobs,
        RunOptions {
            concurrency: 1,
            mode: plan.mode,
            dry_run: false,
        },
        sink.clone(),
        None,
    );
    tokio::time::timeout(Duration::from_secs(180), execute_run(context))
        .await
        .expect("the run finishes");
    let logged: Vec<String> = sink
        .lines
        .lock()
        .unwrap()
        .iter()
        .filter_map(|line| line.strip_prefix("$ ").map(str::to_string))
        .collect();

    (logged, previewed)
}

#[tokio::test(flavor = "multi_thread")]
async fn the_preview_lists_the_npm_lines_a_real_run_logs() {
    let mut install = plan(vec![StepName::Uninstall, StepName::Install]);
    install.packages = vec![PackageSpec::new("left-pad", "^1.3.0")];
    install.extra_install_args = vec!["--cache=npm cache".to_string()];

    let (logged, previewed) = logged_and_previewed_commands(
        r#"{"name":"app","version":"1.0.0","private":true,"dependencies":{"left-pad":"1.2.0"}}"#,
        install,
    )
    .await;

    assert_eq!(
        logged,
        vec![
            "npm uninstall left-pad".to_string(),
            r#"npm install "left-pad@^1.3.0" --no-fund --no-audit "--cache=npm cache""#.to_string(),
        ]
    );
    assert_eq!(previewed, logged);

    let mut version_and_build = plan(vec![StepName::Version, StepName::Build]);
    version_and_build.packages.clear();
    version_and_build.version.bump = VersionBump::Minor;
    let (logged, previewed) = logged_and_previewed_commands(
        r#"{"name":"app","version":"1.0.0","private":true,"scripts":{"build":"node -e \"\""}}"#,
        version_and_build,
    )
    .await;

    assert_eq!(
        logged,
        vec![
            "npm version minor --no-git-tag-version".to_string(),
            "npm run build".to_string(),
        ]
    );
    assert_eq!(previewed, logged);
}

#[tokio::test(flavor = "multi_thread")]
async fn latest_versions_report_a_registry_failure_per_library_without_failing_the_others() {
    let root = TempDir::new("npm-latest-offline");
    root.write("repos/app/.npmrc", OFFLINE_NPMRC);
    root.write(
        "repos/app/package.json",
        r#"{"name":"app","version":"1.0.0","dependencies":{"@acme/missing-one":"^1.0.0","left-pad":"^1.0.0"}}"#,
    );
    let workspace = Workspace::open(root.path()).unwrap();
    let config = UpdateConfig {
        packages: vec![
            PackageSpec::new("@acme/missing-one", "1.0.0"),
            PackageSpec::new("left-pad", "1.0.0"),
        ],
        projects: vec![ConfiguredProject::new("app", "repos/app")],
        ..UpdateConfig::default()
    };
    let drift = read_dependency_drift(&workspace, &config);

    let lookup = LatestLookup {
        limit: 2,
        ..LatestLookup::new(DependencyRequest::new())
    };
    let latest = fetch_latest_versions(&workspace, &config, &drift, &lookup).await;

    let names: Vec<&str> = latest.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(names, vec!["@acme/missing-one", "left-pad"]);
    for entry in &latest {
        assert_eq!(entry.version, None, "{entry:?}");
        let error = entry.error.as_deref().unwrap_or_default();
        assert!(!error.is_empty(), "{entry:?}");
    }
}
