mod common;

use std::sync::{Arc, Mutex, Weak};

use common::TempDir;
use deptide_core::domain::{
    ConfiguredProject, ExecutionMode, JobStatus, PackageSpec, RunPlan, StepName, UpdateConfig,
    VersionPolicy,
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
    let root = TempDir::new("npm-real");
    let tool = root.mkdir("tool");
    root.write(
        "repos/app/package.json",
        r#"{"name":"npm-real-app","version":"1.0.0","private":true,"dependencies":{"left-pad":"1.2.0"}}"#,
    );

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
