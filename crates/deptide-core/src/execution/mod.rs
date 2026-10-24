mod audit;
mod backup;
mod bump;
mod command;
mod diagnose;
mod jobs;
mod logger;
mod manifest;
mod pack;
mod pipeline;
mod preview;
mod process_tree;
mod project_lock;
mod report;
mod scheduler;
mod state;
mod steps;
mod verify;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use chrono::Utc;

pub use audit::{audit_totals, describe_audit, parse_audit_report};
pub use backup::{
    list_run_backups, prune_backups, restore_project, restore_project_locked, restore_run,
    run_name, RestoreRequest,
};
pub use bump::next_version;
pub use command::{
    describe_exit_code, npm_program, run_command, CommandOutcome, CommandSpec, ProcessRegistry,
};
pub use diagnose::diagnose;
pub use jobs::{
    build_command_jobs, build_jobs, collect_package_specs, collect_steps, describe_version_policy,
    nothing_to_do, order_by_dependencies, JobBuildOutcome,
};
pub use logger::{list_summaries, RunLogger};
pub use pack::{mark_packable_candidates, LibraryProject, PackableLibraries};
pub use preview::preview_plan;
pub use process_tree::install_crash_hook;
pub use project_lock::{lock_project, ProjectLock};
pub use report::{render_html, render_markdown};
pub use scheduler::execute_run;
pub use state::{JobState, ProgressSink, RunEvent, RunOptions, RunState, MAX_LOG_LINES};
pub use steps::{plan_install, PlannedCommand};
pub use verify::{
    describe_installed, diff_dependencies, read_top_level_versions, short_integrity,
    verify_installed,
};

use crate::domain::{Job, RunSnapshot};
use crate::error::{AppError, AppResult};
use crate::workspace::normalize_directory;
use project_lock::run_owner;

#[derive(Debug, Clone, Default)]
pub struct RunFolders {
    pub backups: Option<PathBuf>,
    pub locks: Option<PathBuf>,
}

pub struct RunContext {
    pub id: String,
    state: Mutex<RunState>,
    pub sink: Arc<dyn ProgressSink>,
    pub registry: ProcessRegistry,
    pub logger: Option<RunLogger>,
    pub options: RunOptions,
    pub backup_root: Option<PathBuf>,
    pub backup_folder: String,
    locks_directory: Option<PathBuf>,
    project_locks: Mutex<Vec<Option<Arc<ProjectLock>>>>,
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
        Self::with_folders(
            id,
            label,
            jobs,
            options,
            sink,
            logger,
            RunFolders::default(),
        )
    }

    pub fn with_folders(
        id: String,
        label: String,
        jobs: Vec<Job>,
        options: RunOptions,
        sink: Arc<dyn ProgressSink>,
        logger: Option<RunLogger>,
        folders: RunFolders,
    ) -> Arc<Self> {
        let backup_folder = match &logger {
            Some(logger) => logger.run_name().to_string(),
            None => format!("{}-{id}", logger::stamped_run_name(Utc::now(), &label)),
        };
        let mut state = RunState::new(id.clone(), label, jobs, &options);
        state.backup_folder = folders.backups.as_ref().map(|_| backup_folder.clone());

        Arc::new(Self {
            id,
            state: Mutex::new(state),
            sink,
            registry: ProcessRegistry::new(),
            logger,
            options,
            backup_root: folders.backups,
            backup_folder,
            locks_directory: folders.locks,
            project_locks: Mutex::new(Vec::new()),
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
        match &self.locks_directory {
            Some(locks) => restore_project_locked(&backup, &directory, locks),
            None => restore_project(&backup, &directory),
        }
    }

    fn project_locks(&self) -> MutexGuard<'_, Vec<Option<Arc<ProjectLock>>>> {
        self.project_locks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(crate) fn lock_projects(&self) -> Vec<Result<Arc<ProjectLock>, String>> {
        let Some(locks_directory) = self.locks_directory.as_deref() else {
            return Vec::new();
        };
        if self.options.dry_run {
            return Vec::new();
        }

        let (owner, directories) = {
            let state = self.state();
            let directories: Vec<PathBuf> = state
                .jobs
                .iter()
                .map(|job| job.job.directory.clone())
                .collect();
            (run_owner(&state.label), directories)
        };
        let mut by_directory: HashMap<String, Result<Arc<ProjectLock>, String>> = HashMap::new();
        directories
            .iter()
            .map(|directory| {
                by_directory
                    .entry(normalize_directory(directory))
                    .or_insert_with(|| {
                        lock_project(locks_directory, directory, &owner)
                            .map(Arc::new)
                            .map_err(|error| error.message().to_string())
                    })
                    .clone()
            })
            .collect()
    }

    pub(crate) fn keep_project_locks(&self, taken: &[Result<Arc<ProjectLock>, String>]) {
        *self.project_locks() = taken
            .iter()
            .map(|result| result.as_ref().ok().cloned())
            .collect();
    }

    pub(crate) fn project_lock(&self, index: usize) -> Option<Arc<ProjectLock>> {
        self.project_locks().get(index).cloned().flatten()
    }

    pub(crate) fn release_project_lock(&self, index: usize) {
        if let Some(slot) = self.project_locks().get_mut(index) {
            *slot = None;
        }
    }

    pub(crate) fn release_project_locks(&self) {
        self.project_locks().clear();
    }

    pub fn is_finished(&self) -> bool {
        self.state().finished_at_ms.is_some()
    }
}
