mod common;

use std::sync::Arc;

use deptide_core::domain::{ExecutionMode, Job, PackageSpec, StepName, VersionPolicy};
use deptide_core::execution::{execute_run, ProgressSink, RunContext, RunEvent, RunOptions};
use deptide_desktop::app::state::AppState;

struct SilentSink;

impl ProgressSink for SilentSink {
    fn emit(&self, _event: RunEvent) {}
}

fn dry_run_context(id: &str) -> Arc<RunContext> {
    let job = Job {
        name: "web".to_string(),
        directory: std::env::temp_dir(),
        packages: vec![PackageSpec::new("left-pad", "1.3.0")],
        steps: vec![StepName::Install],
        install_args: vec![],
        audit_fix_args: vec![],
        depends_on: vec![],
        command: None,
        version: VersionPolicy::default(),
    };

    RunContext::new(
        id.to_string(),
        "guard".to_string(),
        vec![job],
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
