mod common;

use std::sync::{Arc, Mutex};

use common::{manifest, TempDir};
use deptide_core::domain::{
    BumpWhen, ConfiguredProject, ExecutionMode, JobStatus, PackageSpec, RunPlan, StepName,
    UpdateConfig, VersionBump, VersionPolicy,
};
use deptide_core::execution::{
    build_jobs, execute_run, list_summaries, plan_install, ProgressSink, RunContext, RunEvent,
    RunLogger, RunOptions,
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
        version: VersionPolicy::default(),
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

#[test]
fn build_jobs_gives_every_job_the_version_policy_of_the_plan() {
    let (_root, workspace) = workspace_with_projects();
    let config = deptide_core::workspace::load_config(&workspace).unwrap();
    let mut plan = plan_for(
        &["web", "api"],
        vec![PackageSpec::new("@acme/core", "3.1.0")],
        ExecutionMode::PerProject,
    );
    plan.version = VersionPolicy {
        bump: VersionBump::Major,
        when: BumpWhen::NotBumpedOnBranch,
    };

    let outcome = build_jobs(&workspace, &config, &plan);

    assert_eq!(outcome.jobs.len(), 2);
    for job in &outcome.jobs {
        assert_eq!(job.version, plan.version, "{}", job.name);
    }
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
        version: VersionPolicy::default(),
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

#[tokio::test(flavor = "multi_thread")]
async fn the_version_step_is_skipped_without_a_main_branch_to_compare_with() {
    let root = TempDir::new("version-skip");
    root.write("repos/web/package.json", &manifest("web", "1.0.0", &[]));

    let job = deptide_core::domain::Job {
        name: "web".to_string(),
        directory: root.join("repos/web"),
        packages: vec![PackageSpec::new("left-pad", "1.3.0")],
        steps: vec![StepName::Version],
        install_args: vec![],
        audit_fix_args: vec![],
        depends_on: vec![],
        command: None,
        version: VersionPolicy {
            bump: VersionBump::Minor,
            when: BumpWhen::SameAsMain,
        },
    };

    let sink = Arc::new(RecordingSink::default());
    let context = RunContext::new(
        "run-version".to_string(),
        "version".to_string(),
        vec![job],
        RunOptions {
            concurrency: 1,
            mode: ExecutionMode::PerProject,
            dry_run: false,
        },
        sink.clone(),
        None,
    );

    let snapshot = execute_run(context).await;

    assert_eq!(snapshot.jobs[0].status, JobStatus::Ok);
    assert!(
        std::fs::read_to_string(root.join("repos/web/package.json"))
            .unwrap()
            .contains("1.0.0"),
        "no npm version runs when main cannot be read"
    );
    let logs: Vec<String> = sink
        .events
        .lock()
        .unwrap()
        .iter()
        .filter_map(|event| match event {
            RunEvent::LogLine { line, .. } => Some(line.clone()),
            _ => None,
        })
        .collect();
    assert!(
        logs.iter().any(|line| line.contains("bump skipped")),
        "expected a skip reason in {logs:?}"
    );
}

fn workspace_with_sections() -> (TempDir, Workspace) {
    let root = TempDir::new("sections");
    let tool = root.mkdir("tool");
    root.write(
        "repos/deps/package.json",
        r#"{"name":"deps","dependencies":{"left-pad":"^1.0.0"},"peerDependencies":{"left-pad":"^1.0.0"}}"#,
    );
    root.write(
        "repos/dev/package.json",
        r#"{"name":"dev","devDependencies":{"left-pad":"^1.0.0"}}"#,
    );
    root.write(
        "repos/peer/package.json",
        r#"{"name":"peer","peerDependencies":{"left-pad":"^1.0.0"}}"#,
    );
    root.write(
        "repos/peer-dev/package.json",
        r#"{"name":"peer-dev","peerDependencies":{"left-pad":"^1.0.0"},"devDependencies":{"left-pad":"^1.2.0"}}"#,
    );

    let workspace = Workspace::open(&tool).expect("workspace");
    let config = UpdateConfig {
        projects: ["deps", "dev", "peer", "peer-dev"]
            .iter()
            .map(|name| ConfiguredProject::new(*name, format!("../repos/{name}")))
            .collect(),
        ..UpdateConfig::default()
    };
    save_config(&workspace, &config).expect("config saved");

    (root, workspace)
}

#[test]
fn build_jobs_installs_each_package_from_the_section_the_project_declares_it_in() {
    let (_root, workspace) = workspace_with_sections();
    let config = deptide_core::workspace::load_config(&workspace).unwrap();

    let outcome = build_jobs(
        &workspace,
        &config,
        &plan_for(
            &["deps", "dev", "peer", "peer-dev"],
            vec![PackageSpec::new("left-pad", "1.3.0")],
            ExecutionMode::PerProject,
        ),
    );

    assert!(
        outcome.without_packages.is_empty(),
        "a peer-only project declares the package: {:?}",
        outcome.without_packages
    );
    let flags: Vec<(&str, bool, bool)> = outcome
        .jobs
        .iter()
        .map(|job| {
            let spec = &job.packages[0];
            (job.name.as_str(), spec.save_dev, spec.save_peer)
        })
        .collect();

    assert_eq!(
        flags,
        vec![
            ("deps", false, false),
            ("dev", true, false),
            ("peer", false, true),
            ("peer-dev", true, false),
        ]
    );
}

fn install_args_per_group(job: &deptide_core::domain::Job) -> Vec<Vec<String>> {
    plan_install(job, false)
        .into_iter()
        .map(|command| command.args)
        .collect()
}

#[test]
fn build_jobs_installs_without_a_flag_when_the_package_is_also_in_dependencies() {
    let root = TempDir::new("sections-with-dependencies");
    let tool = root.mkdir("tool");
    root.write(
        "repos/deps-dev/package.json",
        r#"{"name":"deps-dev","dependencies":{"left-pad":"^1.0.0"},"devDependencies":{"left-pad":"^1.2.0"}}"#,
    );
    root.write(
        "repos/all/package.json",
        r#"{"name":"all","dependencies":{"left-pad":"^1.0.0"},"devDependencies":{"left-pad":"^1.2.0"},"peerDependencies":{"left-pad":"^1.0.0"}}"#,
    );
    let workspace = Workspace::open(&tool).expect("workspace");
    let config = UpdateConfig {
        projects: vec![
            ConfiguredProject::new("deps-dev", "../repos/deps-dev"),
            ConfiguredProject::new("all", "../repos/all"),
        ],
        ..UpdateConfig::default()
    };

    let outcome = build_jobs(
        &workspace,
        &config,
        &plan_for(
            &["deps-dev", "all"],
            vec![PackageSpec::new("left-pad", "1.3.0")],
            ExecutionMode::PerProject,
        ),
    );

    let names: Vec<&str> = outcome.jobs.iter().map(|job| job.name.as_str()).collect();
    assert_eq!(names, vec!["deps-dev", "all"]);
    for job in &outcome.jobs {
        assert_eq!(
            install_args_per_group(job),
            vec![vec!["install", "left-pad@1.3.0", "--force"]],
            "{}",
            job.name
        );
    }
}

#[test]
fn a_package_flagged_dev_and_peer_installs_as_dev_where_there_is_no_package_json() {
    let root = TempDir::new("sections-no-manifest");
    let tool = root.mkdir("tool");
    root.mkdir("repos/bare");
    let workspace = Workspace::open(&tool).expect("workspace");
    let config = UpdateConfig {
        projects: vec![ConfiguredProject::new("bare", "../repos/bare")],
        ..UpdateConfig::default()
    };
    let package = PackageSpec {
        save_dev: true,
        save_peer: true,
        ..PackageSpec::new("left-pad", "1.3.0")
    };

    let outcome = build_jobs(
        &workspace,
        &config,
        &plan_for(&["bare"], vec![package], ExecutionMode::PerProject),
    );

    assert_eq!(outcome.jobs.len(), 1);
    assert_eq!(
        install_args_per_group(&outcome.jobs[0]),
        vec![vec!["install", "left-pad@1.3.0", "--save-dev", "--force"]]
    );
}

#[test]
fn peer_installs_turn_save_exact_off_and_other_installs_keep_it() {
    let root = TempDir::new("save-exact");
    let exact_flags = ["--save-exact", "-E", "--save-exact=true"];
    let job = deptide_core::domain::Job {
        name: "lib".to_string(),
        directory: root.join("repos/lib"),
        packages: vec![
            PackageSpec::new("left-pad", "1.3.0"),
            PackageSpec {
                save_dev: true,
                ..PackageSpec::new("typescript", "5.0.0")
            },
            PackageSpec {
                save_peer: true,
                ..PackageSpec::new("@acme/core", "2.1.0")
            },
        ],
        steps: vec![StepName::Install],
        install_args: exact_flags
            .iter()
            .map(|flag| flag.to_string())
            .chain(["--no-fund".to_string()])
            .collect(),
        audit_fix_args: vec![],
        depends_on: vec![],
        command: None,
        version: VersionPolicy::default(),
    };

    let groups = install_args_per_group(&job);

    assert_eq!(
        groups,
        vec![
            vec![
                "install",
                "left-pad@1.3.0",
                "--save-exact",
                "-E",
                "--save-exact=true",
                "--no-fund"
            ],
            vec![
                "install",
                "typescript@5.0.0",
                "--save-dev",
                "--save-exact",
                "-E",
                "--save-exact=true",
                "--no-fund"
            ],
            vec![
                "install",
                "@acme/core@2.1.0",
                "--save-peer",
                "--no-fund",
                "--no-save-exact"
            ],
        ]
    );
}

fn peer_job(root: &TempDir, steps: Vec<StepName>) -> deptide_core::domain::Job {
    root.write(
        "repos/lib/package.json",
        r#"{"name":"lib","peerDependencies":{"left-pad":"^1.0.0","@acme/core":"^1 || ^2"},"devDependencies":{"left-pad":"^1.2.0"}}"#,
    );

    deptide_core::domain::Job {
        name: "lib".to_string(),
        directory: root.join("repos/lib"),
        packages: vec![
            PackageSpec {
                save_dev: true,
                ..PackageSpec::new("left-pad", "1.3.0")
            },
            PackageSpec {
                save_peer: true,
                ..PackageSpec::new("@acme/core", "2.1.0")
            },
        ],
        steps,
        install_args: vec!["--save-exact".to_string()],
        audit_fix_args: vec![],
        depends_on: vec![],
        command: None,
        version: VersionPolicy::default(),
    }
}

async fn dry_run_log(jobs: Vec<deptide_core::domain::Job>, mode: ExecutionMode) -> Vec<String> {
    let context = RunContext::new(
        "run-peer".to_string(),
        "peer".to_string(),
        jobs,
        RunOptions {
            concurrency: 1,
            mode,
            dry_run: true,
        },
        Arc::new(RecordingSink::default()),
        None,
    );

    execute_run(context.clone()).await;
    context.snapshot(true).jobs.remove(0).log
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dry_run_shows_peer_installs_and_the_peer_ranges_it_would_restore() {
    for mode in [ExecutionMode::PerProject, ExecutionMode::PerStep] {
        let root = TempDir::new("peer-dry-run");
        let job = peer_job(
            &root,
            vec![StepName::Uninstall, StepName::Install, StepName::Build],
        );
        let log = dry_run_log(vec![job], mode).await;

        for expected in [
            "would install left-pad@1.3.0 --save-dev",
            "would install @acme/core@2.1.0 --save-peer",
            "--save-exact dropped for peer installs: an exact peer range pins consumers to one version",
            "would restore peerDependencies.left-pad = ^1.0.0",
        ] {
            assert!(
                log.iter().any(|line| line == expected),
                "{mode:?}: expected {expected:?} in {log:?}"
            );
        }
        assert!(
            !log.iter()
                .any(|line| line.contains("peerDependencies.@acme/core")),
            "{mode:?}: a package installed with --save-peer is not restored: {log:?}"
        );

        let restore = log
            .iter()
            .position(|line| line.starts_with("would restore"))
            .unwrap();
        let last_install = log
            .iter()
            .rposition(|line| line.starts_with("would install"))
            .unwrap();
        let build = log
            .iter()
            .position(|line| line == "would run npm run build")
            .unwrap();
        assert!(
            last_install < restore && restore < build,
            "{mode:?}: restore follows the last package step: {log:?}"
        );
        assert_eq!(
            log.iter()
                .filter(|line| line.starts_with("would restore"))
                .count(),
            1,
            "{mode:?}: {log:?}"
        );
    }
}

const SAVE_EXACT_NOTE: &str =
    "--save-exact dropped for peer installs: an exact peer range pins consumers to one version";

#[tokio::test(flavor = "multi_thread")]
async fn a_dry_run_notes_the_dropped_save_exact_for_each_spelling_of_the_flag() {
    for flag in ["--save-exact", "-E", "--save-exact=true"] {
        let root = TempDir::new("peer-note");
        let mut job = peer_job(&root, vec![StepName::Install]);
        job.install_args = vec![flag.to_string()];
        let log = dry_run_log(vec![job], ExecutionMode::PerProject).await;

        assert!(
            log.iter().any(|line| line == SAVE_EXACT_NOTE),
            "{flag}: {log:?}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dry_run_without_save_exact_has_no_note() {
    let root = TempDir::new("peer-no-note");
    let mut job = peer_job(&root, vec![StepName::Install]);
    job.install_args = vec!["--no-fund".to_string()];
    let log = dry_run_log(vec![job], ExecutionMode::PerProject).await;

    assert!(!log.iter().any(|line| line == SAVE_EXACT_NOTE), "{log:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_without_package_steps_neither_snapshots_nor_restores_peer_ranges() {
    let root = TempDir::new("peer-no-package-steps");
    let job = peer_job(&root, vec![StepName::Build]);
    let log = dry_run_log(vec![job], ExecutionMode::PerProject).await;

    assert!(
        !log.iter().any(|line| line.contains("peerDependencies")),
        "{log:?}"
    );
}

struct StopJobAtPhase {
    context: Mutex<std::sync::Weak<RunContext>>,
    job: String,
    phase: StepName,
}

impl ProgressSink for StopJobAtPhase {
    fn emit(&self, event: RunEvent) {
        if let RunEvent::PhaseChanged {
            phase: Some(phase), ..
        } = event
        {
            if phase == self.phase {
                if let Some(context) = self.context.lock().unwrap().upgrade() {
                    context.abort_job(&self.job).unwrap();
                }
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_per_step_dry_run_stopped_before_its_install_phase_lists_every_saved_entry() {
    let root = TempDir::new("peer-per-step-stop");
    let job = peer_job(&root, vec![StepName::Uninstall, StepName::Install]);
    let sink = Arc::new(StopJobAtPhase {
        context: Mutex::new(std::sync::Weak::new()),
        job: "lib".to_string(),
        phase: StepName::Install,
    });
    let context = RunContext::new(
        "run-stop".to_string(),
        "stop".to_string(),
        vec![job],
        RunOptions {
            concurrency: 1,
            mode: ExecutionMode::PerStep,
            dry_run: true,
        },
        sink.clone(),
        None,
    );
    *sink.context.lock().unwrap() = Arc::downgrade(&context);

    execute_run(context.clone()).await;
    let snapshot = context.snapshot(true);
    let log = &snapshot.jobs[0].log;

    assert_eq!(snapshot.jobs[0].status, JobStatus::Skipped);
    let mut restores: Vec<&str> = log
        .iter()
        .filter(|line| line.starts_with("would restore"))
        .map(String::as_str)
        .collect();
    restores.sort();
    assert_eq!(
        restores,
        vec![
            "would restore devDependencies.left-pad = ^1.2.0",
            "would restore peerDependencies.@acme/core = ^1 || ^2",
            "would restore peerDependencies.left-pad = ^1.0.0",
        ],
        "{log:?}"
    );
}
