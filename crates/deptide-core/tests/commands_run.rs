mod common;

use std::sync::{Arc, Mutex};

use common::{manifest, TempDir};
use deptide_core::domain::{ConfiguredProject, ExecutionMode, JobStatus, UpdateConfig};
use deptide_core::execution::{
    build_command_jobs, execute_run, ProgressSink, RunContext, RunEvent, RunOptions,
};
use deptide_core::workspace::Workspace;

#[derive(Default)]
struct Lines {
    seen: Mutex<Vec<(String, String)>>,
}

impl ProgressSink for Lines {
    fn emit(&self, event: RunEvent) {
        if let RunEvent::LogLine { job, line, .. } = event {
            self.seen.lock().unwrap().push((job, line));
        }
    }
}

fn workspace_with(root: &TempDir, names: &[&str]) -> (Workspace, UpdateConfig) {
    for name in names {
        root.write(
            &format!("repos/{name}/package.json"),
            &manifest(name, "1.0.0", &[]),
        );
    }
    let tool = root.mkdir("tool");
    let workspace = Workspace::open(&tool).unwrap();
    let config = UpdateConfig {
        projects: names
            .iter()
            .map(|name| ConfiguredProject::new(*name, format!("../repos/{name}")))
            .collect(),
        ..UpdateConfig::default()
    };
    (workspace, config)
}

#[test]
fn command_jobs_carry_the_line_and_report_unknown_projects() {
    let root = TempDir::new("command-jobs");
    let (workspace, config) = workspace_with(&root, &["web", "api"]);

    let outcome = build_command_jobs(
        &workspace,
        &config,
        &["web".to_string(), "ghost".to_string()],
        "echo hi",
    );

    assert_eq!(outcome.jobs.len(), 1);
    assert_eq!(outcome.jobs[0].command.as_deref(), Some("echo hi"));
    assert!(outcome.jobs[0].steps.is_empty());
    assert_eq!(outcome.missing, vec!["ghost"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn shell_commands_run_in_every_project_and_exit_codes_become_failures() {
    let root = TempDir::new("command-run");
    let (workspace, config) = workspace_with(&root, &["web", "api"]);

    let ok_jobs = build_command_jobs(
        &workspace,
        &config,
        &["web".to_string(), "api".to_string()],
        "echo hello from %CD%",
    )
    .jobs;
    let sink = Arc::new(Lines::default());
    let context = RunContext::new(
        "cmd-1".to_string(),
        "cmd: echo".to_string(),
        ok_jobs,
        RunOptions {
            concurrency: 2,
            mode: ExecutionMode::PerProject,
            dry_run: false,
        },
        sink.clone(),
        None,
    );
    let snapshot = execute_run(context.clone()).await;

    assert!(
        snapshot.jobs.iter().all(|job| job.status == JobStatus::Ok),
        "{:?}",
        snapshot.jobs
    );
    assert_eq!(snapshot.command.as_deref(), Some("echo hello from %CD%"));
    {
        let lines = sink.seen.lock().unwrap();
        assert!(lines
            .iter()
            .any(|(job, line)| job == "web" && line.starts_with("hello from")));
        assert!(lines
            .iter()
            .any(|(job, line)| job == "api" && line.contains("exit code 0")));
    }

    let failing = build_command_jobs(&workspace, &config, &["web".to_string()], "exit 3").jobs;
    let context = RunContext::new(
        "cmd-2".to_string(),
        "cmd: exit".to_string(),
        failing,
        RunOptions {
            concurrency: 1,
            mode: ExecutionMode::PerProject,
            dry_run: false,
        },
        Arc::new(Lines::default()),
        None,
    );
    let snapshot = execute_run(context).await;

    assert_eq!(snapshot.jobs[0].status, JobStatus::Failed);
    assert!(snapshot.jobs[0].error.contains("code 3"));
    assert_eq!(snapshot.summary.unwrap().failed_count, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dry_run_only_announces_the_command() {
    let root = TempDir::new("command-dry");
    let (workspace, config) = workspace_with(&root, &["web"]);
    let jobs = build_command_jobs(&workspace, &config, &["web".to_string()], "npm run lint").jobs;

    let context = RunContext::new(
        "cmd-3".to_string(),
        "cmd".to_string(),
        jobs,
        RunOptions {
            concurrency: 1,
            mode: ExecutionMode::PerProject,
            dry_run: true,
        },
        Arc::new(Lines::default()),
        None,
    );
    execute_run(context.clone()).await;

    let snapshot = context.snapshot(true);
    assert_eq!(snapshot.jobs[0].status, JobStatus::Ok);
    assert!(snapshot.jobs[0]
        .log
        .iter()
        .any(|line| line == "would run npm run lint"));
}
