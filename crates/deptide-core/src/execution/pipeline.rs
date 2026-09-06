use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use super::backup::backup_project;
use super::command::{describe_exit_code, npm_program, run_command, CommandOutcome, CommandSpec};
use super::diagnose::diagnose;
use super::state::{JobState, RunEvent};
use super::steps::{
    dry_run_lines, plan_audit_fix, plan_build, plan_install, plan_uninstall, PlannedCommand,
};
use super::verify::{
    describe_installed, diff_dependencies, read_top_level_versions, verify_installed,
};
use super::RunContext;
use crate::domain::{Job, JobStatus, StepName};
use crate::scan::{read_project_at, MANIFEST_FILE_NAME};
use crate::util::text::describe_count;

const TRANSIENT_CODES: &[&str] = &["ENETWORK", "EINTEGRITY"];
const MAX_ATTEMPTS: u32 = 2;

enum StepError {
    Aborted,
    Failure(String),
}

pub struct JobRunner {
    context: Arc<RunContext>,
    index: usize,
}

impl JobRunner {
    pub fn new(context: Arc<RunContext>, index: usize) -> Self {
        Self { context, index }
    }

    fn with_state<R>(&self, update: impl FnOnce(&mut JobState) -> R) -> R {
        let mut run = self.context.state();
        update(&mut run.jobs[self.index])
    }

    fn job(&self) -> Job {
        self.with_state(|state| state.job.clone())
    }

    fn emit_job(&self) {
        let job = self.with_state(|state| state.snapshot(false));
        self.context.sink.emit(RunEvent::JobChanged {
            run_id: self.context.id.clone(),
            job,
        });
    }

    fn append_log(&self, line: String) {
        let job_name = self.with_state(|state| {
            state.push_log(line.clone());
            state.job.name.clone()
        });

        if let Some(logger) = &self.context.logger {
            logger.write(&job_name, &line);
        }

        self.context.sink.emit(RunEvent::LogLine {
            run_id: self.context.id.clone(),
            job: job_name,
            line,
        });
    }

    fn start_step(&self, label: &str) -> PathBuf {
        let directory = self.with_state(|state| {
            state.current_step = label.to_string();
            state.job.directory.clone()
        });
        self.emit_job();
        directory
    }

    async fn run_in_project(&self, spec: CommandSpec) -> Result<CommandOutcome, StepError> {
        let outcome = run_command(&spec, &self.context.registry, self.index, |line| {
            self.append_log(line)
        })
        .await;

        if outcome.killed || self.context.registry.is_job_aborted(self.index) {
            return Err(StepError::Aborted);
        }

        Ok(outcome)
    }

    async fn run_npm(&self, command: PlannedCommand) -> Result<CommandOutcome, StepError> {
        let directory = self.start_step(&command.label);
        self.append_log(format!("$ npm {}", command.args.join(" ")));

        let outcome = self
            .run_in_project(CommandSpec {
                program: npm_program().to_string(),
                args: command.args,
                cwd: directory,
                shell: None,
            })
            .await?;

        if let Some(error) = outcome.spawn_error {
            return Err(StepError::Failure(format!(
                "npm could not be started: {error}"
            )));
        }

        Ok(outcome)
    }

    async fn run_shell_command(&self, line: &str) -> Result<(), StepError> {
        let directory = self.start_step(line);
        self.append_log(format!("$ {line}"));

        if self.context.options.dry_run {
            self.append_log(format!("would run {line}"));
            return Ok(());
        }

        let started = Instant::now();
        let outcome = run_command(
            &CommandSpec {
                program: String::new(),
                args: Vec::new(),
                cwd: directory,
                shell: Some(line.to_string()),
            },
            &self.context.registry,
            self.index,
            |line| self.append_log(line),
        )
        .await;
        let elapsed = started.elapsed().as_millis() as u64;
        self.append_log(format!(
            "finished in {} ms with exit code {}",
            elapsed,
            describe_exit_code(outcome.code)
        ));

        if outcome.killed || self.context.registry.is_job_aborted(self.index) {
            return Err(StepError::Aborted);
        }
        if let Some(error) = outcome.spawn_error {
            return Err(StepError::Failure(format!(
                "command could not be started: {error}"
            )));
        }
        if outcome.code != Some(0) {
            return Err(StepError::Failure(format!(
                "command exited with code {}",
                describe_exit_code(outcome.code)
            )));
        }

        Ok(())
    }

    async fn step_uninstall(&self, job: &Job) -> Result<(), StepError> {
        self.run_npm(plan_uninstall(job)).await?;
        Ok(())
    }

    async fn step_install(&self, job: &Job, force: bool) -> Result<(), StepError> {
        let before = read_top_level_versions(&job.directory);

        for command in plan_install(job, force) {
            let outcome = self.run_npm(command).await?;
            if outcome.code != Some(0) {
                return Err(StepError::Failure(format!(
                    "npm install exited with code {}",
                    describe_exit_code(outcome.code)
                )));
            }
        }

        self.verify(job);
        self.record_dependency_changes(job, &before);
        Ok(())
    }

    async fn step_audit(&self, job: &Job) -> Result<(), StepError> {
        let outcome = self.run_npm(plan_audit_fix(job)).await?;
        if outcome.code != Some(0) {
            self.with_state(|state| {
                state.warning = "audit fix left unresolved vulnerabilities".to_string();
            });
        }

        Ok(())
    }

    async fn step_build(&self, job: &Job) -> Result<(), StepError> {
        let has_build_script = read_project_at(&job.directory, &job.directory)
            .map(|project| project.has_build_script)
            .unwrap_or(false);

        if !has_build_script {
            self.append_log("no build script, step skipped".to_string());
            return Ok(());
        }

        let outcome = self.run_npm(plan_build()).await?;
        if outcome.code != Some(0) {
            return Err(StepError::Failure(format!(
                "npm run build exited with code {}",
                describe_exit_code(outcome.code)
            )));
        }

        Ok(())
    }

    fn verify(&self, job: &Job) {
        let installed = verify_installed(&job.directory, &job.packages);
        let mismatches: Vec<String> = installed
            .iter()
            .filter(|entry| !entry.matches)
            .map(|entry| {
                format!(
                    "{} is {} instead of {}",
                    entry.name,
                    entry.installed.as_deref().unwrap_or("missing"),
                    entry.expected
                )
            })
            .collect();

        for entry in &installed {
            self.append_log(describe_installed(entry));
        }

        self.with_state(|state| {
            state.installed = installed;
            if !mismatches.is_empty() {
                state.warning = format!("installed version differs: {}", mismatches.join(", "));
            }
        });
    }

    fn record_dependency_changes(&self, job: &Job, before: &BTreeMap<String, String>) {
        let after = read_top_level_versions(&job.directory);
        let changes = diff_dependencies(before, &after);

        if !changes.is_empty() {
            self.append_log(format!(
                "{} changed in node_modules",
                describe_count(changes.len(), None, "dependencies")
            ));
        }

        self.with_state(|state| state.dependency_changes = changes);
    }

    fn back_up_files(&self, job: &Job) {
        let Some(root) = &self.context.backup_root else {
            return;
        };

        let already = self.with_state(|state| state.backup_directory.is_some());
        if already {
            return;
        }

        match backup_project(root, &self.context.backup_folder, &job.name, &job.directory) {
            Ok(Some(directory)) => {
                self.append_log(format!(
                    "backed up package.json and lockfile to {}",
                    directory.display()
                ));
                self.with_state(|state| state.backup_directory = Some(directory));
            }
            Ok(None) => {}
            Err(error) => self.append_log(format!("backup skipped: {error}")),
        }
    }

    pub fn mark_skipped_with_reason(&self, reason: String) {
        self.with_state(|state| {
            state.status = JobStatus::Skipped;
            state.error = reason;
        });
        self.emit_job();
    }

    async fn run_step_once(&self, job: &Job, step: StepName) -> Result<(), StepError> {
        if self.context.options.dry_run {
            for line in dry_run_lines(job, step) {
                self.append_log(line);
            }
            return Ok(());
        }

        match step {
            StepName::Uninstall => self.step_uninstall(job).await,
            StepName::Install => self.step_install(job, false).await,
            StepName::ForceInstall => self.step_install(job, true).await,
            StepName::Audit => self.step_audit(job).await,
            StepName::Build => self.step_build(job).await,
        }
    }

    fn is_transient_failure(&self) -> bool {
        self.with_state(|state| diagnose(&state.log))
            .is_some_and(|diagnosis| TRANSIENT_CODES.contains(&diagnosis.code.as_str()))
    }

    async fn run_step_with_retry(&self, job: &Job, step: StepName) -> Result<(), StepError> {
        let started = Instant::now();
        let mut result = self.run_step_once(job, step).await;

        for attempt in 2..=MAX_ATTEMPTS {
            let retry = matches!(result, Err(StepError::Failure(_))) && self.is_transient_failure();
            if !retry {
                break;
            }

            self.append_log(format!(
                "transient error detected, retrying {} (attempt {attempt} of {MAX_ATTEMPTS})",
                step.label()
            ));
            self.with_state(|state| state.retries += 1);
            result = self.run_step_once(job, step).await;
        }

        let duration_ms = started.elapsed().as_millis() as u64;
        self.with_state(|state| state.record_step(step, duration_ms));

        result
    }

    fn ensure_manifest(job: &Job) -> Result<(), StepError> {
        if job.directory.join(MANIFEST_FILE_NAME).exists() {
            Ok(())
        } else {
            Err(StepError::Failure(format!(
                "no {MANIFEST_FILE_NAME} in {}",
                job.directory.display()
            )))
        }
    }

    fn apply_failure(&self, error: StepError) {
        self.with_state(|state| match error {
            StepError::Aborted => {
                state.status = JobStatus::Skipped;
                if state.error.is_empty() {
                    state.error = "stopped before it finished".to_string();
                }
            }
            StepError::Failure(message) => {
                state.status = JobStatus::Failed;
                state.error = message;
                state.diagnosis = diagnose(&state.log);
            }
        });
    }

    fn finish_with(&self, result: Result<(), StepError>, on_success: JobStatus) {
        match result {
            Ok(()) => self.with_state(|state| {
                state.status = if on_success == JobStatus::Ok && !state.warning.is_empty() {
                    JobStatus::Warn
                } else {
                    on_success
                };
            }),
            Err(error) => self.apply_failure(error),
        }
        self.end();
    }

    fn begin(&self) -> bool {
        if self.context.registry.is_job_aborted(self.index) {
            self.mark_skipped_with_reason("stopped before it started".to_string());
            return false;
        }

        self.with_state(JobState::begin);
        self.emit_job();
        true
    }

    fn end(&self) {
        self.with_state(JobState::end);
        self.emit_job();
    }

    pub async fn run_all(&self) {
        if !self.begin() {
            return;
        }

        let job = self.job();

        if let Some(line) = job.command.clone() {
            let result = self.run_shell_command(&line).await;
            self.finish_with(result, JobStatus::Ok);
            return;
        }

        let mut result = Self::ensure_manifest(&job);

        if result.is_ok() {
            self.back_up_files(&job);
            for step in &job.steps {
                result = self.run_step_with_retry(&job, *step).await;
                if result.is_err() {
                    break;
                }
            }
        }

        self.finish_with(result, JobStatus::Ok);
    }

    pub async fn run_step(&self, step: StepName) {
        let (is_final, has_step, is_command) = self.with_state(|state| {
            (
                state.status.is_final(),
                state.job.steps.contains(&step),
                state.job.command.is_some(),
            )
        });
        if is_final || !has_step || is_command {
            return;
        }

        if !self.begin() {
            return;
        }

        let job = self.job();
        let result = match Self::ensure_manifest(&job) {
            Ok(()) => {
                self.back_up_files(&job);
                self.run_step_with_retry(&job, step).await
            }
            Err(error) => Err(error),
        };

        self.finish_with(result, JobStatus::Pending);
    }
}
