use std::future::Future;
use std::sync::{Arc, Mutex};

use tokio::sync::{Notify, Semaphore};
use tokio::task::JoinSet;

use super::pipeline::JobRunner;
use super::state::{finalize_states, RunEvent};
use super::RunContext;
use crate::domain::{ExecutionMode, JobStatus, RunSnapshot, StepName};

struct DependencyGate {
    done: Mutex<Vec<bool>>,
    notify: Notify,
}

impl DependencyGate {
    fn new(job_count: usize) -> Arc<Self> {
        Arc::new(Self {
            done: Mutex::new(vec![false; job_count]),
            notify: Notify::new(),
        })
    }

    async fn wait_for(&self, dependencies: &[usize]) {
        loop {
            let notified = self.notify.notified();
            let ready = {
                let done = self
                    .done
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                dependencies.iter().all(|index| done[*index])
            };
            if ready {
                return;
            }
            notified.await;
        }
    }

    fn mark_done(&self, index: usize) {
        self.done
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())[index] = true;
        self.notify.notify_waiters();
    }
}

fn dependency_indices(context: &RunContext) -> Vec<Vec<usize>> {
    let state = context.state();
    let names: Vec<&str> = state.jobs.iter().map(|job| job.job.name.as_str()).collect();

    state
        .jobs
        .iter()
        .map(|job| {
            job.job
                .depends_on
                .iter()
                .filter_map(|name| names.iter().position(|known| known == name))
                .collect()
        })
        .collect()
}

fn failed_dependency(context: &RunContext, dependencies: &[usize]) -> Option<String> {
    let state = context.state();

    dependencies
        .iter()
        .map(|index| &state.jobs[*index])
        .find(|job| matches!(job.status, JobStatus::Failed | JobStatus::Skipped))
        .map(|job| job.job.name.clone())
}

async fn for_each_job<F, Fut>(context: &Arc<RunContext>, limit: usize, work: F)
where
    F: Fn(JobRunner) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let dependencies = dependency_indices(context);
    let gate = DependencyGate::new(dependencies.len());
    let semaphore = Arc::new(Semaphore::new(limit.max(1)));
    let work = Arc::new(work);
    let mut tasks = JoinSet::new();

    for (index, wait_on) in dependencies.into_iter().enumerate() {
        let semaphore = semaphore.clone();
        let context = context.clone();
        let work = work.clone();
        let gate = gate.clone();

        tasks.spawn(async move {
            gate.wait_for(&wait_on).await;
            let runner = JobRunner::new(context.clone(), index);

            match failed_dependency(&context, &wait_on) {
                Some(dependency) => {
                    runner.mark_skipped_with_reason(format!(
                        "not run because {dependency} did not succeed"
                    ));
                }
                None => {
                    if let Ok(_permit) = semaphore.acquire_owned().await {
                        work(runner).await;
                    }
                }
            }

            gate.mark_done(index);
        });
    }

    while tasks.join_next().await.is_some() {}
}

fn set_phase(context: &Arc<RunContext>, phase: Option<StepName>) {
    context.state().current_phase = phase;

    if let (Some(step), Some(logger)) = (phase, &context.logger) {
        logger.note(&format!("--- step {} ---", step.label()));
    }

    context.sink.emit(RunEvent::PhaseChanged {
        run_id: context.id.clone(),
        phase,
    });
}

async fn run_per_project(context: &Arc<RunContext>, limit: usize) {
    for_each_job(context, limit, |runner| async move {
        runner.run_all().await;
    })
    .await;
}

async fn run_per_step(context: &Arc<RunContext>, limit: usize) {
    let steps = context.state().steps.clone();

    for step in steps {
        set_phase(context, Some(step));
        for_each_job(context, limit, move |runner| async move {
            runner.run_step(step).await;
        })
        .await;
    }

    set_phase(context, None);
    finalize_states(&mut context.state().jobs);
}

fn finish(context: &Arc<RunContext>) -> RunSnapshot {
    let aborted = context.registry.is_aborted();
    let log_file = context
        .logger
        .as_ref()
        .map(|logger| logger.log_file().to_string_lossy().to_string());

    let summary = context.state().finish(aborted, log_file);

    if let Some(logger) = &context.logger {
        let _ = logger.close(&summary);
    }

    let snapshot = context.snapshot(false);
    context.sink.emit(RunEvent::Finished {
        run_id: context.id.clone(),
        snapshot: snapshot.clone(),
    });

    snapshot
}

pub async fn execute_run(context: Arc<RunContext>) -> RunSnapshot {
    let limit = context.options.concurrency.max(1) as usize;

    match context.options.mode {
        ExecutionMode::PerProject => run_per_project(&context, limit).await,
        ExecutionMode::PerStep => run_per_step(&context, limit).await,
    }

    finish(&context)
}
