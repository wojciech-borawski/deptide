pub mod exec;
pub mod history;
pub mod run;
pub mod scan;

use std::sync::Arc;

use deptide_core::domain::{ConfiguredProject, Job, RunSnapshot, RunSummary};
use deptide_core::error::{AppError, AppResult};
use deptide_core::execution::{execute_run, RunContext, RunLogger, RunOptions};
use deptide_core::util::time::{format_duration, now_ms};
use deptide_core::workspace::Workspace;

use crate::exit_code::ExitCode;
use crate::sink::TerminalSink;

pub struct Launch {
    pub label: String,
    pub jobs: Vec<Job>,
    pub options: RunOptions,
    pub log_header: Vec<String>,
    pub backups: Option<std::path::PathBuf>,
    pub show_output: bool,
    pub print_json: bool,
}

pub fn default_project_names(projects: &[ConfiguredProject], requested: &[String]) -> Vec<String> {
    if !requested.is_empty() {
        return requested.to_vec();
    }
    projects
        .iter()
        .filter(|project| !project.skip)
        .map(|project| project.name.clone())
        .collect()
}

pub async fn launch(workspace: &Workspace, launch: Launch) -> AppResult<ExitCode> {
    let logger = RunLogger::open(
        &workspace.logs_directory(),
        &launch.label,
        &launch.log_header,
    )?;
    let context = RunContext::with_backups(
        format!("cli-{}", now_ms()),
        launch.label,
        launch.jobs,
        launch.options,
        Arc::new(TerminalSink::new(launch.show_output)),
        Some(logger),
        launch.backups,
    );

    let stopper = context.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            eprintln!("stopping…");
            stopper.abort();
        }
    });

    let snapshot = execute_run(context).await;
    let summary = snapshot
        .summary
        .as_ref()
        .ok_or_else(|| AppError::new("The run ended without a summary"))?;

    if launch.print_json {
        println!("{}", serde_json::to_string_pretty(summary)?);
    } else {
        print_summary(&snapshot, summary);
    }

    Ok(ExitCode::from_summary(summary))
}

fn print_summary(snapshot: &RunSnapshot, summary: &RunSummary) {
    println!();
    println!(
        "{}: {} ok, {} warnings, {} failed, {} skipped in {} ({} of work)",
        snapshot.label,
        summary.ok_count,
        summary.warn_count,
        summary.failed_count,
        summary.skipped_count,
        format_duration(summary.total_duration_ms),
        format_duration(summary.busy_duration_ms)
    );
    if let Some(log_file) = &summary.log_file {
        println!("transcript: {log_file}");
    }
}
