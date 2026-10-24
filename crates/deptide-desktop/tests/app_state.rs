mod common;

use std::sync::Arc;

use common::{manifest, TempDir};
use deptide_core::domain::{ExecutionMode, Job, PackageSpec, StepName, VersionPolicy};
use deptide_core::execution::{
    execute_run, lock_project, ProgressSink, RunContext, RunEvent, RunFolders, RunOptions,
};
use deptide_desktop::app::state::AppState;

struct SilentSink;

impl ProgressSink for SilentSink {
    fn emit(&self, _event: RunEvent) {}
}

fn dry_run_context(id: &str) -> Arc<RunContext> {
    dry_run_context_in(id, std::env::temp_dir())
}

fn dry_run_job(directory: std::path::PathBuf) -> Job {
    Job {
        name: "web".to_string(),
        directory,
        packages: vec![PackageSpec::new("left-pad", "1.3.0")],
        steps: vec![StepName::Install],
        install_args: vec![],
        audit_fix_args: vec![],
        depends_on: vec![],
        command: None,
        version: VersionPolicy::default(),
        packing: None,
    }
}

fn dry_run_context_in(id: &str, directory: std::path::PathBuf) -> Arc<RunContext> {
    RunContext::new(
        id.to_string(),
        "guard".to_string(),
        vec![dry_run_job(directory)],
        RunOptions {
            concurrency: 1,
            mode: ExecutionMode::PerProject,
            dry_run: true,
        },
        Arc::new(SilentSink),
        None,
    )
}

#[test]
fn run_ids_are_unique_and_increasing() {
    let state = AppState::default();
    let first = state.next_run_id();
    let second = state.next_run_id();

    assert_ne!(first, second);
    assert!(first.starts_with("run-"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_registered_run_counts_as_active_until_it_finishes() {
    let state = AppState::default();
    assert!(state.active_run().is_none());

    let context = dry_run_context("run-a");
    state.register(context.clone());
    assert_eq!(
        state.active_run().map(|run| run.id.clone()),
        Some("run-a".to_string())
    );
    assert!(state.get("run-a").is_some());
    assert!(state.get("run-b").is_none());

    execute_run(context).await;
    assert!(
        state.active_run().is_none(),
        "a finished run no longer blocks new runs"
    );
}

#[test]
fn abort_all_marks_every_registered_run_as_aborted() {
    let state = AppState::default();
    let first = dry_run_context("run-a");
    let second = dry_run_context("run-b");
    state.register(first.clone());
    state.register(second.clone());

    state.abort_all();

    assert!(first.registry.is_aborted());
    assert!(second.registry.is_aborted());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_project_is_not_restored_while_another_run_works_in_it() {
    let root = TempDir::new("restore-while-busy");
    let original = manifest("web", "1.0.0", &[]);
    root.write("web/package.json", &original);
    let state = AppState::default();
    let finished = RunContext::with_folders(
        "run-a".to_string(),
        "real".to_string(),
        vec![Job {
            steps: vec![StepName::Build],
            packages: vec![],
            ..dry_run_job(root.join("web"))
        }],
        RunOptions {
            concurrency: 1,
            mode: ExecutionMode::PerProject,
            dry_run: false,
        },
        Arc::new(SilentSink),
        None,
        RunFolders {
            backups: Some(root.join("backups")),
            locks: Some(root.join("locks")),
        },
    );
    state.register(finished.clone());
    execute_run(finished.clone()).await;
    root.write("web/package.json", "{\"changed\":true}");
    let other_run = lock_project(&root.join("locks"), &root.join("web"), "the other run")
        .expect("another run holds the project");

    let refused = finished.restore_job_files("web");

    assert!(refused
        .unwrap_err()
        .message()
        .starts_with("Not restored: the other run is working in this project"));
    assert_eq!(
        std::fs::read_to_string(root.join("web/package.json")).unwrap(),
        "{\"changed\":true}"
    );
    drop(other_run);
    assert_eq!(
        finished.restore_job_files("web").unwrap(),
        vec!["package.json"]
    );
    assert_eq!(
        std::fs::read_to_string(root.join("web/package.json")).unwrap(),
        original
    );
}
