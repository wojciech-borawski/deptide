mod backup;
mod command;
mod diagnose;
mod jobs;
mod logger;
mod pipeline;
mod report;
mod scheduler;
mod state;
mod steps;
mod verify;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use chrono::Utc;

pub use backup::{prune_backups, restore_project};
pub use command::{npm_program, ProcessRegistry};
pub use diagnose::diagnose;
pub use jobs::{
    build_command_jobs, build_jobs, collect_package_specs, collect_steps, order_by_dependencies,
    JobBuildOutcome,
};
pub use logger::{list_summaries, RunLogger};
pub use report::{render_html, render_markdown};
pub use scheduler::execute_run;
pub use state::{JobState, ProgressSink, RunEvent, RunOptions, RunState, MAX_LOG_LINES};
pub use verify::{
    describe_installed, diff_dependencies, read_top_level_versions, short_integrity,
    verify_installed,
};

use crate::domain::{Job, RunSnapshot};
use crate::error::{AppError, AppResult};
use crate::util::text::slugify;
use crate::util::time::file_stamp;

pub struct RunContext {
    pub id: String,
    state: Mutex<RunState>,
    pub sink: Arc<dyn ProgressSink>,
    pub registry: ProcessRegistry,
    pub logger: Option<RunLogger>,
    pub options: RunOptions,
    pub backup_root: Option<PathBuf>,
    pub backup_folder: String,
}

impl RunContext {
    pub fn new(
        id: String,
        label: String,
        jobs: Vec<Job>,
        options: RunOptions,
        sink: Arc<dyn ProgressSink>,
        logger: Option<RunLogger>,
    ) -> Arc<Self> {
        Self::with_backups(id, label, jobs, options, sink, logger, None)
    }

    pub fn with_backups(
        id: String,
        label: String,
        jobs: Vec<Job>,
        options: RunOptions,
        sink: Arc<dyn ProgressSink>,
        logger: Option<RunLogger>,
        backup_root: Option<PathBuf>,
    ) -> Arc<Self> {
        let suffix = slugify(&label);
        let backup_folder = if suffix.is_empty() {
            file_stamp(Utc::now())
        } else {
            format!("{}-{suffix}", file_stamp(Utc::now()))
        };
        let state = RunState::new(id.clone(), label, jobs, &options);

        Arc::new(Self {
            id,
            state: Mutex::new(state),
            sink,
            registry: ProcessRegistry::new(),
            logger,
            options,
            backup_root,
            backup_folder,
        })
    }

    pub fn state(&self) -> MutexGuard<'_, RunState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn snapshot(&self, include_log: bool) -> RunSnapshot {
        self.state().snapshot(include_log)
    }

    pub fn abort(&self) {
        self.registry.abort_all();
    }

    pub fn job_index(&self, job_name: &str) -> AppResult<usize> {
        self.state()
            .jobs
            .iter()
            .position(|job| job.job.name == job_name)
            .ok_or_else(|| AppError::new(format!("Unknown project {job_name}")))
    }

    pub fn abort_job(&self, job_name: &str) -> AppResult<()> {
        let index = self.job_index(job_name)?;
        self.registry.abort_job(index);
        Ok(())
    }

    pub fn restore_job_files(&self, job_name: &str) -> AppResult<Vec<String>> {
        let index = self.job_index(job_name)?;
        let (backup, directory) = {
            let state = self.state();
            let job = &state.jobs[index];
            (job.backup_directory.clone(), job.job.directory.clone())
        };

        let backup = backup.ok_or_else(|| AppError::new("No backup was taken for this project"))?;
        restore_project(&backup, &directory)
    }

    pub fn is_finished(&self) -> bool {
        self.state().finished_at_ms.is_some()
    }
}
