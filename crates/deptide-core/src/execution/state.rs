use std::path::PathBuf;
use std::time::Instant;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::domain::{
    DependencyChange, Diagnosis, ExecutionMode, InstalledPackage, Job, JobSnapshot, JobStatus,
    RunProjectSummary, RunSnapshot, RunSummary, StepName, StepTiming,
};
use crate::util::time::{iso_timestamp, now_ms};

pub const MAX_LOG_LINES: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunOptions {
    pub concurrency: u32,
    pub mode: ExecutionMode,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RunEvent {
    #[serde(rename_all = "camelCase")]
    JobChanged { run_id: String, job: JobSnapshot },
    #[serde(rename_all = "camelCase")]
    LogLine {
        run_id: String,
        job: String,
        line: String,
    },
    #[serde(rename_all = "camelCase")]
    PhaseChanged {
        run_id: String,
        phase: Option<StepName>,
    },
    #[serde(rename_all = "camelCase")]
    Finished {
        run_id: String,
        snapshot: RunSnapshot,
    },
}

pub trait ProgressSink: Send + Sync {
    fn emit(&self, event: RunEvent);
}

#[derive(Debug)]
pub struct JobState {
    pub job: Job,
    pub status: JobStatus,
    pub current_step: String,
    pub started_at_ms: Option<u64>,
    pub step_started_at_ms: Option<u64>,
    started_instant: Option<Instant>,
    pub duration_ms: u64,
    pub step_timings: Vec<StepTiming>,
    pub log: Vec<String>,
    pub error: String,
    pub warning: String,
    pub installed: Vec<InstalledPackage>,
    pub diagnosis: Option<Diagnosis>,
    pub dependency_changes: Vec<DependencyChange>,
    pub retries: u32,
    pub backup_directory: Option<PathBuf>,
}

impl JobState {
    pub fn new(job: Job) -> Self {
        Self {
            job,
            status: JobStatus::Pending,
            current_step: String::new(),
            started_at_ms: None,
            step_started_at_ms: None,
            started_instant: None,
            duration_ms: 0,
            step_timings: Vec::new(),
            log: Vec::new(),
            error: String::new(),
            warning: String::new(),
            installed: Vec::new(),
            diagnosis: None,
            dependency_changes: Vec::new(),
            retries: 0,
            backup_directory: None,
        }
    }

    pub fn begin(&mut self) {
        self.status = JobStatus::Running;
        self.started_at_ms.get_or_insert_with(now_ms);
        self.step_started_at_ms = Some(now_ms());
        self.started_instant = Some(Instant::now());
    }

    pub fn end(&mut self) {
        if let Some(started) = self.started_instant.take() {
            self.duration_ms += started.elapsed().as_millis() as u64;
        }
        self.step_started_at_ms = None;
        self.current_step.clear();
    }

    pub fn record_step(&mut self, step: StepName, duration_ms: u64) {
        self.step_timings.push(StepTiming { step, duration_ms });
    }

    pub fn push_log(&mut self, line: String) {
        self.log.push(line);
        if self.log.len() > MAX_LOG_LINES {
            let excess = self.log.len() - MAX_LOG_LINES;
            self.log.drain(..excess);
        }
    }

    pub fn snapshot(&self, include_log: bool) -> JobSnapshot {
        JobSnapshot {
            name: self.job.name.clone(),
            directory: self.job.directory.to_string_lossy().to_string(),
            packages: self
                .job
                .packages
                .iter()
                .map(|spec| spec.to_spec())
                .collect(),
            status: self.status,
            current_step: self.current_step.clone(),
            started_at_ms: self.started_at_ms,
            step_started_at_ms: self.step_started_at_ms,
            duration_ms: self.duration_ms,
            step_timings: self.step_timings.clone(),
            error: self.error.clone(),
            warning: self.warning.clone(),
            depends_on: self.job.depends_on.clone(),
            installed: self.installed.clone(),
            diagnosis: self.diagnosis.clone(),
            dependency_changes: self.dependency_changes.clone(),
            retries: self.retries,
            backup_available: self.backup_directory.is_some(),
            log: if include_log {
                self.log.clone()
            } else {
                Vec::new()
            },
        }
    }

    fn summary(&self) -> RunProjectSummary {
        RunProjectSummary {
            name: self.job.name.clone(),
            directory: self.job.directory.to_string_lossy().to_string(),
            status: self.status,
            duration_ms: self.duration_ms,
            step_timings: self.step_timings.clone(),
            error: self.error.clone(),
            warning: self.warning.clone(),
            installed: self.installed.clone(),
            diagnosis: self.diagnosis.clone(),
            dependency_changes: self.dependency_changes.clone(),
            retries: self.retries,
        }
    }
}

#[derive(Debug)]
pub struct RunState {
    pub id: String,
    pub label: String,
    pub packages: Vec<String>,
    pub steps: Vec<StepName>,
    pub mode: ExecutionMode,
    pub concurrency: u32,
    pub dry_run: bool,
    pub started_at: DateTime<Utc>,
    pub started_at_ms: u64,
    started_instant: Instant,
    pub finished_at_ms: Option<u64>,
    pub current_phase: Option<StepName>,
    pub aborted: bool,
    pub jobs: Vec<JobState>,
    pub summary: Option<RunSummary>,
}

impl RunState {
    pub fn new(id: String, label: String, jobs: Vec<Job>, options: &RunOptions) -> Self {
        Self {
            id,
            label,
            packages: super::collect_package_specs(&jobs),
            steps: super::collect_steps(&jobs),
            mode: options.mode,
            concurrency: options.concurrency,
            dry_run: options.dry_run,
            started_at: Utc::now(),
            started_at_ms: now_ms(),
            started_instant: Instant::now(),
            finished_at_ms: None,
            current_phase: None,
            aborted: false,
            jobs: jobs.into_iter().map(JobState::new).collect(),
            summary: None,
        }
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.started_instant.elapsed().as_millis() as u64
    }

    pub fn snapshot(&self, include_log: bool) -> RunSnapshot {
        RunSnapshot {
            id: self.id.clone(),
            label: self.label.clone(),
            packages: self.packages.clone(),
            steps: self.steps.clone(),
            mode: self.mode,
            concurrency: self.concurrency,
            dry_run: self.dry_run,
            started_at_ms: self.started_at_ms,
            finished_at_ms: self.finished_at_ms,
            current_phase: self.current_phase,
            aborted: self.aborted,
            command: self.jobs.first().and_then(|job| job.job.command.clone()),
            jobs: self
                .jobs
                .iter()
                .map(|job| job.snapshot(include_log))
                .collect(),
            summary: self.summary.clone(),
        }
    }

    pub fn finish(&mut self, aborted: bool, log_file: Option<String>) -> RunSummary {
        let finished_at = Utc::now();
        self.finished_at_ms = Some(now_ms());
        self.aborted = aborted;

        let count = |status: JobStatus| self.jobs.iter().filter(|job| job.status == status).count();

        let summary = RunSummary {
            label: self.label.clone(),
            packages: self.packages.clone(),
            project_count: self.jobs.len(),
            concurrency: self.concurrency,
            steps: self.steps.clone(),
            mode: self.mode,
            dry_run: self.dry_run,
            started_at: iso_timestamp(self.started_at),
            finished_at: iso_timestamp(finished_at),
            total_duration_ms: self.elapsed_ms(),
            busy_duration_ms: self.jobs.iter().map(|job| job.duration_ms).sum(),
            log_file,
            projects: self.jobs.iter().map(JobState::summary).collect(),
            ok_count: count(JobStatus::Ok),
            warn_count: count(JobStatus::Warn),
            failed_count: count(JobStatus::Failed),
            skipped_count: count(JobStatus::Skipped),
            aborted,
        };

        self.summary = Some(summary.clone());
        summary
    }
}

pub fn finalize_states(states: &mut [JobState]) {
    for state in states {
        if !matches!(state.status, JobStatus::Pending | JobStatus::Running) {
            continue;
        }

        state.status = if state.step_timings.is_empty() {
            JobStatus::Skipped
        } else if state.warning.is_empty() {
            JobStatus::Ok
        } else {
            JobStatus::Warn
        };
    }
}
