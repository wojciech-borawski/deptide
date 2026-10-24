pub mod deps;
pub mod exec;
pub mod groups;
pub mod history;
pub mod restore;
pub mod run;
pub mod scan;

use std::sync::Arc;

use deptide_core::domain::{
    AuditCounts, ConfiguredProject, Job, RunSnapshot, RunSummary, UpdateConfig,
};
use deptide_core::error::{AppError, AppResult};
use deptide_core::execution::{
    audit_totals, describe_audit, execute_run, RunContext, RunFolders, RunLogger, RunOptions,
};
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

pub fn select_project_names(
    config: &UpdateConfig,
    requested: &[String],
    group_names: &[String],
) -> AppResult<Vec<String>> {
    if group_names.is_empty() {
        return Ok(default_project_names(&config.projects, requested));
    }

    let mut names = requested.to_vec();
    for group_name in group_names {
        let group = config
            .groups
            .iter()
            .find(|group| &group.name == group_name)
            .ok_or_else(|| AppError::new(unknown_group_message(config, group_name)))?;
        for name in &group.projects {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
    }

    Ok(names)
}

fn unknown_group_message(config: &UpdateConfig, group_name: &str) -> String {
    if config.groups.is_empty() {
        return format!("Unknown group {group_name}: update-libs.json defines no groups");
    }
    let known: Vec<&str> = config
        .groups
        .iter()
        .map(|group| group.name.as_str())
        .collect();
    format!(
        "Unknown group {group_name}; known groups: {}",
        known.join(", ")
    )
}

pub async fn launch(workspace: &Workspace, launch: Launch) -> AppResult<ExitCode> {
    let logger = RunLogger::open(
        &workspace.logs_directory(),
        &launch.label,
        &launch.log_header,
    )?;
    let context = RunContext::with_folders(
        format!("cli-{}", now_ms()),
        launch.label,
        launch.jobs,
        launch.options,
        Arc::new(TerminalSink::new(launch.show_output)),
        Some(logger),
        RunFolders {
            backups: launch.backups,
            locks: Some(workspace.locks_directory()),
        },
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
    for line in audit_lines(summary) {
        println!("{line}");
    }
    if let Some(log_file) = &summary.log_file {
        println!("transcript: {log_file}");
    }
}

fn audit_lines(summary: &RunSummary) -> Vec<String> {
    let audited: Vec<(&str, &AuditCounts)> = summary
        .projects
        .iter()
        .filter_map(|project| {
            project
                .audit
                .as_ref()
                .map(|audit| (project.name.as_str(), audit))
        })
        .collect();
    let totals = audit_totals(audited.iter().map(|(_, audit)| *audit));

    audited
        .iter()
        .map(|(name, audit)| format!("{name} {}", describe_audit(audit)))
        .chain(totals.map(|totals| format!("all projects {}", describe_audit(&totals))))
        .collect()
}

#[cfg(test)]
mod tests {
    use deptide_core::domain::RunSummary;

    use super::audit_lines;

    fn summary(audits: &[(&str, &str)]) -> RunSummary {
        let projects: Vec<String> = audits
            .iter()
            .map(|(name, audit)| {
                format!(
                    r#"{{"name":"{name}","directory":"","status":"ok","durationMs":1,"stepTimings":[],"error":"","warning":""{audit}}}"#
                )
            })
            .collect();
        serde_json::from_str(&format!(
            r#"{{"label":"run","packages":[],"projectCount":{},"concurrency":1,"steps":["audit-report"],
            "mode":"per-project","dryRun":false,"startedAt":"","finishedAt":"","totalDurationMs":1,
            "busyDurationMs":1,"logFile":null,"okCount":1,"warnCount":0,"failedCount":0,
            "skippedCount":0,"aborted":false,"projects":[{}]}}"#,
            projects.len(),
            projects.join(",")
        ))
        .unwrap()
    }

    #[test]
    fn the_summary_lists_audit_counts_per_project_and_in_total() {
        let summary = summary(&[
            (
                "web",
                r#","audit":{"info":0,"low":0,"moderate":0,"high":1,"critical":1,"total":2}"#,
            ),
            ("docs", ""),
            (
                "api",
                r#","audit":{"info":1,"low":0,"moderate":2,"high":0,"critical":0,"total":3}"#,
            ),
        ]);

        assert_eq!(
            audit_lines(&summary),
            vec![
                "web audit: 2 vulnerabilities (1 critical, 1 high, 0 moderate, 0 low, 0 info)",
                "api audit: 3 vulnerabilities (0 critical, 0 high, 2 moderate, 0 low, 1 info)",
                "all projects audit: 5 vulnerabilities (1 critical, 1 high, 2 moderate, 0 low, 1 info)",
            ]
        );
        assert!(audit_lines(&self::summary(&[("docs", "")])).is_empty());
    }
}
