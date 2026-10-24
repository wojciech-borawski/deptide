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

struct JobSettled {
    context: Arc<RunContext>,
    gate: Arc<DependencyGate>,
    index: usize,
}

impl Drop for JobSettled {
    fn drop(&mut self) {
        if std::thread::panicking() {
            let job = &mut self.context.state().jobs[self.index];
            job.end();
            job.status = JobStatus::Failed;
            job.error = "stopped by an internal error".to_string();
        }
        self.gate.mark_done(self.index);
    }
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
            let _lock = context.project_lock(index);
            let _settled = JobSettled {
                context: context.clone(),
                gate: gate.clone(),
                index,
            };
            let runner = JobRunner::new(context.clone(), index);
            let already_ended = context.state().jobs[index].status.is_final();

            match failed_dependency(&context, &wait_on) {
                _ if already_ended => {}
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

struct ReleasedWhenDone<'a>(&'a JobRunner);

impl Drop for ReleasedWhenDone<'_> {
    fn drop(&mut self) {
        self.0.release_project_lock();
    }
}

async fn run_per_project(context: &Arc<RunContext>, limit: usize) {
    for_each_job(context, limit, |runner| async move {
        let _release = ReleasedWhenDone(&runner);
        runner.run_all().await;
    })
    .await;
}

struct HeldProjectLocks<'a>(&'a RunContext);

impl Drop for HeldProjectLocks<'_> {
    fn drop(&mut self) {
        self.0.release_project_locks();
    }
}

async fn hold_project_locks(context: &Arc<RunContext>) -> HeldProjectLocks<'_> {
    let locking = context.clone();
    let taken = match tokio::task::spawn_blocking(move || locking.lock_projects()).await {
        Ok(taken) => taken,
        Err(error) => std::panic::resume_unwind(error.into_panic()),
    };
    context.keep_project_locks(&taken);
    let held = HeldProjectLocks(context);
    for (index, result) in taken.into_iter().enumerate() {
        if let Err(reason) = result {
            JobRunner::new(context.clone(), index)
                .mark_skipped_with_reason(format!("not run because {reason}"));
        }
    }
    held
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
    let job_count = context.state().jobs.len();
    for index in 0..job_count {
        JobRunner::new(context.clone(), index)
            .restore_pending_entries()
            .await;
    }
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

    {
        let _locks = hold_project_locks(&context).await;
        match context.options.mode {
            ExecutionMode::PerProject => run_per_project(&context, limit).await,
            ExecutionMode::PerStep => run_per_step(&context, limit).await,
        }
    }

    finish(&context)
}
