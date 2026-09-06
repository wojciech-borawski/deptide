mod common;

use std::sync::{Arc, Mutex};

use common::{manifest, TempDir};
use deptide_core::domain::{
    ConfiguredProject, ExecutionMode, JobStatus, PackageSpec, RunPlan, StepName, UpdateConfig,
};
use deptide_core::execution::{
    build_jobs, execute_run, list_summaries, ProgressSink, RunContext, RunEvent, RunLogger,
    RunOptions,
};
use deptide_core::workspace::{save_config, Workspace};

#[derive(Default)]
struct RecordingSink {
    events: Mutex<Vec<RunEvent>>,
}

impl ProgressSink for RecordingSink {
    fn emit(&self, event: RunEvent) {
        self.events.lock().unwrap().push(event);
    }
}

fn plan_for(names: &[&str], packages: Vec<PackageSpec>, mode: ExecutionMode) -> RunPlan {
    RunPlan {
        project_names: names.iter().map(|name| name.to_string()).collect(),
        packages,
        steps: StepName::default_steps().to_vec(),
        mode,
        concurrency: 2,
        dry_run: true,
        extra_install_args: vec!["--force".to_string()],
        label: "test run".to_string(),
        save_as: None,
    }
}

fn workspace_with_projects() -> (TempDir, Workspace) {
    let root = TempDir::new("execution");
    let tool = root.mkdir("tool");
    root.write(
        "repos/web/package.json",
        &manifest("web", "1.0.0", &[("@acme/core", "^3.0.0")]),
    );
    root.write(
        "repos/api/package.json",
        r#"{"name":"api","version":"1.0.0","devDependencies":{"@acme/core":"3.0.0"}}"#,
    );
    root.mkdir("repos/empty");

    let workspace = Workspace::open(&tool).expect("workspace");
    let config = UpdateConfig {
        projects: vec![
            ConfiguredProject::new("web", "../repos/web"),
            ConfiguredProject::new("api", "../repos/api"),
            ConfiguredProject::new("empty", "../repos/empty"),
        ],
        install_args: vec!["--no-fund".to_string()],
        ..UpdateConfig::default()
    };
    save_config(&workspace, &config).expect("config saved");

    (root, workspace)
}

#[test]
fn build_jobs_applies_packages_only_where_they_are_declared() {
    let (_root, workspace) = workspace_with_projects();
    let config = deptide_core::workspace::load_config(&workspace).unwrap();

    let outcome = build_jobs(
        &workspace,
        &config,
        &plan_for(
            &["web", "api", "ghost"],
            vec![
                PackageSpec::new("@acme/core", "3.1.0-ABC-1"),
                PackageSpec::new("@acme/extra", "1.0.0"),
            ],
            ExecutionMode::PerProject,
        ),
    );

    assert_eq!(outcome.missing, vec!["ghost"]);
    assert_eq!(outcome.jobs.len(), 2);

    let web = &outcome.jobs[0];
    assert_eq!(web.packages.len(), 2);
    assert!(!web.packages[0].save_dev);
    assert_eq!(web.install_args, vec!["--no-fund", "--force"]);

    let api = &outcome.jobs[1];
    assert_eq!(api.packages.len(), 2);
    assert!(api.packages[0].save_dev, "declared under devDependencies");
}

#[test]
fn build_jobs_skips_projects_where_nothing_applies() {
    let (_root, workspace) = workspace_with_projects();
    let mut config = deptide_core::workspace::load_config(&workspace).unwrap();
    config.projects[0].packages = Some(vec!["@acme/other".to_string()]);

    let outcome = build_jobs(
        &workspace,
        &config,
        &plan_for(
            &["web", "api"],
            vec![PackageSpec::new("@acme/core", "3.1.0")],
            ExecutionMode::PerProject,
        ),
    );

    assert_eq!(outcome.without_packages, vec!["web"]);
    assert_eq!(outcome.jobs.len(), 1);
}

async fn run(
    mode: ExecutionMode,
    abort_first: bool,
) -> (TempDir, Arc<RunContext>, Arc<RecordingSink>) {
    let (root, workspace) = workspace_with_projects();
    let config = deptide_core::workspace::load_config(&workspace).unwrap();
    let plan = plan_for(
        &["web", "api", "empty"],
        vec![PackageSpec::new("@acme/core", "3.1.0-ABC-1")],
        mode,
    );
    let jobs = build_jobs(&workspace, &config, &plan).jobs;
    assert_eq!(jobs.len(), 3);

    let sink = Arc::new(RecordingSink::default());
    let logger = RunLogger::open(
        &workspace.logs_directory(),
        &plan.label,
        &["header".to_string()],
    )
    .unwrap();
    let context = RunContext::new(
        "run-1".to_string(),
        plan.label.clone(),
        jobs,
        RunOptions {
            concurrency: 2,
            mode,
            dry_run: true,
        },
        sink.clone(),
        Some(logger),
    );

    if abort_first {
        context.abort();
    }

    execute_run(context.clone()).await;
    (root, context, sink)
}

#[tokio::test(flavor = "multi_thread")]
async fn dry_run_per_project_reports_statuses_timings_and_total_time() {
    let (root, context, sink) = run(ExecutionMode::PerProject, false).await;
    let snapshot = context.snapshot(true);

    assert!(snapshot.finished_at_ms.is_some());
    assert!(!snapshot.aborted);

    let statuses: Vec<JobStatus> = snapshot.jobs.iter().map(|job| job.status).collect();
    assert_eq!(
        statuses,
        vec![JobStatus::Ok, JobStatus::Ok, JobStatus::Failed]
    );
    assert_eq!(snapshot.jobs[0].step_timings.len(), 4);
    assert!(snapshot.jobs[0]
        .log
        .iter()
        .any(|line| line == "would uninstall @acme/core"));
    assert!(snapshot.jobs[2].error.contains("no package.json"));

    let summary = snapshot.summary.expect("summary present");
    assert_eq!((summary.ok_count, summary.failed_count), (2, 1));
    assert_eq!(summary.project_count, 3);
    assert!(summary.total_duration_ms < 60_000);
    assert!(summary.log_file.is_some());

    let events = sink.events.lock().unwrap();
    assert!(matches!(events.last(), Some(RunEvent::Finished { .. })));
    assert!(events
        .iter()
        .any(|event| matches!(event, RunEvent::LogLine { .. })));

    let history = list_summaries(&root.join("tool/logs"));
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].label, "test run");
}

#[tokio::test(flavor = "multi_thread")]
async fn dry_run_per_step_moves_every_project_through_each_phase() {
    let (_root, context, sink) = run(ExecutionMode::PerStep, false).await;
    let snapshot = context.snapshot(false);

    assert_eq!(snapshot.jobs[0].status, JobStatus::Ok);
    assert_eq!(snapshot.jobs[1].status, JobStatus::Ok);
    assert_eq!(snapshot.jobs[2].status, JobStatus::Failed);
    assert_eq!(snapshot.jobs[0].step_timings.len(), 4);

    let phases: Vec<Option<StepName>> = sink
        .events
        .lock()
        .unwrap()
        .iter()
        .filter_map(|event| match event {
            RunEvent::PhaseChanged { phase, .. } => Some(*phase),
            _ => None,
        })
        .collect();

    assert_eq!(
        phases,
        vec![
            Some(StepName::Uninstall),
            Some(StepName::Install),
            Some(StepName::Audit),
            Some(StepName::Build),
            None
        ]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn force_install_step_adds_the_force_flag() {
    let (_root, workspace) = workspace_with_projects();
    let config = deptide_core::workspace::load_config(&workspace).unwrap();
    let mut plan = plan_for(
        &["web"],
        vec![PackageSpec::new("@acme/core", "3.1.0-ABC-1")],
        ExecutionMode::PerProject,
    );
    plan.steps = vec![StepName::ForceInstall];
    plan.extra_install_args = vec![];

    let jobs = build_jobs(&workspace, &config, &plan).jobs;
    let sink = Arc::new(RecordingSink::default());
    let context = RunContext::new(
        "run-force".to_string(),
        plan.label.clone(),
        jobs,
        RunOptions {
            concurrency: 1,
            mode: ExecutionMode::PerProject,
            dry_run: true,
        },
        sink,
        None,
    );

    execute_run(context.clone()).await;
    let snapshot = context.snapshot(true);
    let job = &snapshot.jobs[0];

    assert_eq!(job.status, JobStatus::Ok);
    assert_eq!(job.step_timings.len(), 1);
    assert_eq!(job.step_timings[0].step, StepName::ForceInstall);
    assert!(job
        .log
        .iter()
        .any(|line| line == "would install @acme/core@3.1.0-ABC-1 --force"));
    assert_eq!(snapshot.steps, vec![StepName::ForceInstall]);
}

#[test]
fn jobs_are_ordered_so_libraries_come_before_their_consumers() {
    let root = TempDir::new("ordering");
    let tool = root.mkdir("tool");
    root.write(
        "repos/web/package.json",
        &manifest(
            "web",
            "1.0.0",
            &[("@acme/core", "^3.0.0"), ("@acme/ui", "^1.0.0")],
        ),
    );
    root.write(
        "repos/ui/package.json",
        &manifest("@acme/ui", "1.0.0", &[("@acme/core", "^3.0.0")]),
    );
    root.write(
        "repos/core/package.json",
        &manifest("@acme/core", "3.0.0", &[]),
    );

    let workspace = Workspace::open(&tool).expect("workspace");
    let config = UpdateConfig {
        projects: vec![
            ConfiguredProject::new("web", "../repos/web"),
            ConfiguredProject::new("ui", "../repos/ui"),
            ConfiguredProject::new("core", "../repos/core"),
        ],
        ..UpdateConfig::default()
    };

    let outcome = build_jobs(
        &workspace,
        &config,
        &plan_for(
            &["web", "ui", "core"],
            vec![PackageSpec::new("left-pad", "1.3.0")],
            ExecutionMode::PerProject,
        ),
    );

    let names: Vec<&str> = outcome.jobs.iter().map(|job| job.name.as_str()).collect();
    assert_eq!(names, vec!["core", "ui", "web"]);
    assert_eq!(outcome.jobs[2].depends_on, vec!["core", "ui"]);
    assert_eq!(outcome.jobs[1].depends_on, vec!["core"]);
    assert!(outcome.jobs[0].depends_on.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_consumer_is_skipped_when_its_library_fails() {
    let root = TempDir::new("dependency-skip");
    root.write("repos/web/package.json", &manifest("web", "1.0.0", &[]));

    let job = |name: &str, directory: &str, depends_on: Vec<&str>| deptide_core::domain::Job {
        name: name.to_string(),
        directory: root.join(directory),
        packages: vec![PackageSpec::new("left-pad", "1.3.0")],
        steps: vec![StepName::Install],
        install_args: vec![],
        audit_fix_args: vec![],
        depends_on: depends_on.into_iter().map(String::from).collect(),
        command: None,
    };

    let sink = Arc::new(RecordingSink::default());
    let context = RunContext::new(
        "run-deps".to_string(),
        "deps".to_string(),
        vec![
            job("core", "repos/missing", vec![]),
            job("web", "repos/web", vec!["core"]),
        ],
        RunOptions {
            concurrency: 3,
            mode: ExecutionMode::PerProject,
            dry_run: true,
        },
        sink,
        None,
    );

    let snapshot = execute_run(context).await;

    assert_eq!(snapshot.jobs[0].status, JobStatus::Failed);
    assert_eq!(snapshot.jobs[1].status, JobStatus::Skipped);
    assert!(snapshot.jobs[1].error.contains("core did not succeed"));
    assert_eq!(snapshot.jobs[1].depends_on, vec!["core"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_aborted_run_skips_everything() {
    let (_root, context, _sink) = run(ExecutionMode::PerProject, true).await;
    let snapshot = context.snapshot(false);

    assert!(snapshot.aborted);
    assert!(snapshot
        .jobs
        .iter()
        .all(|job| job.status == JobStatus::Skipped));
    assert_eq!(snapshot.summary.unwrap().skipped_count, 3);
}
