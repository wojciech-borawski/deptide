use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use tauri::{AppHandle, State};

use crate::app::dto::RunStartOutcome;
use crate::app::events::TauriSink;
use crate::app::state::AppState;
use crate::app::tray;
use deptide_core::domain::{
    ExecutionMode, Job, RunPlan, RunSnapshot, RunSummary, SavedRun, StepName,
};
use deptide_core::error::{AppError, AppResult};
use deptide_core::execution::{
    build_command_jobs, build_jobs, collect_package_specs, collect_steps, execute_run,
    prune_backups, render_html, render_markdown, JobBuildOutcome, RunContext, RunLogger,
    RunOptions,
};
use deptide_core::util::time::iso_timestamp;
use deptide_core::workspace::{open_with_config, save_run, Workspace};

struct LaunchRequest {
    label: String,
    jobs: Vec<Job>,
    options: RunOptions,
    log_header: Vec<String>,
    backups: Option<PathBuf>,
}

fn ensure_no_active_run(state: &AppState) -> AppResult<()> {
    if state.active_run().is_some() {
        return Err(AppError::new("A run is already in progress"));
    }
    Ok(())
}

fn launch_run(
    app: AppHandle,
    state: &AppState,
    workspace: &Workspace,
    request: LaunchRequest,
) -> AppResult<(String, RunSnapshot)> {
    let logger = RunLogger::open(
        &workspace.logs_directory(),
        &request.label,
        &request.log_header,
    )?;

    let run_id = state.next_run_id();
    tray::set_running(&app, &request.label);
    if let Some(backups) = &request.backups {
        prune_backups(backups);
    }

    let context = RunContext::with_backups(
        run_id.clone(),
        request.label,
        request.jobs,
        request.options,
        Arc::new(TauriSink::new(app)),
        Some(logger),
        request.backups,
    );

    state.register(context.clone());
    let snapshot = context.snapshot(true);
    tauri::async_runtime::spawn(execute_run(context));

    Ok((run_id, snapshot))
}

fn describe_mode(mode: ExecutionMode) -> &'static str {
    match mode {
        ExecutionMode::PerProject => "project by project",
        ExecutionMode::PerStep => "step by step",
    }
}

fn update_log_header(
    plan: &RunPlan,
    packages: &[String],
    steps: &[StepName],
    job_count: usize,
) -> Vec<String> {
    let mut header = vec![
        format!("label:       {}", plan.label),
        format!("packages:    {}", packages.join(", ")),
        format!("projects:    {job_count}"),
        format!(
            "steps:       {}",
            steps
                .iter()
                .map(|step| step.label())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        format!("order:       {}", describe_mode(plan.mode)),
        format!("concurrency: {}", plan.concurrency),
    ];

    if plan.dry_run {
        header.push("mode:        dry run".to_string());
    }

    header
}

fn save_plan_if_requested(workspace: &Workspace, plan: &RunPlan) -> AppResult<Option<String>> {
    let Some(name) = plan
        .save_as
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
    else {
        return Ok(None);
    };

    let saved = SavedRun {
        name: name.to_string(),
        saved_at: iso_timestamp(Utc::now()),
        projects: plan.project_names.clone(),
        packages: plan.packages.clone(),
        steps: plan.steps.clone(),
        concurrency: plan.concurrency,
        mode: plan.mode,
        extra_install_args: plan.extra_install_args.clone(),
    };

    save_run(workspace, &saved).map(Some)
}

fn start_outcome(
    run_id: String,
    snapshot: RunSnapshot,
    outcome: JobBuildOutcome,
    saved_run_path: Option<String>,
) -> RunStartOutcome {
    RunStartOutcome {
        run_id,
        snapshot,
        missing: outcome.missing,
        without_packages: outcome.without_packages,
        saved_run_path,
    }
}

#[tauri::command]
pub async fn start_run(
    app: AppHandle,
    state: State<'_, AppState>,
    root: String,
    plan: RunPlan,
) -> AppResult<RunStartOutcome> {
    ensure_no_active_run(&state)?;

    let (workspace, config) = open_with_config(&root)?;
    let mut outcome = build_jobs(&workspace, &config, &plan);

    if outcome.jobs.is_empty() {
        return Err(AppError::new(
            "Nothing to do: none of the selected projects uses the chosen packages",
        ));
    }

    let saved_run_path = save_plan_if_requested(&workspace, &plan)?;
    let packages = collect_package_specs(&outcome.jobs);
    let steps = collect_steps(&outcome.jobs);
    let jobs = std::mem::take(&mut outcome.jobs);

    let request = LaunchRequest {
        label: plan.label.clone(),
        log_header: update_log_header(&plan, &packages, &steps, jobs.len()),
        jobs,
        options: RunOptions {
            concurrency: plan.concurrency.max(1),
            mode: plan.mode,
            dry_run: plan.dry_run,
        },
        backups: (!plan.dry_run).then(|| workspace.backups_directory()),
    };

    let (run_id, snapshot) = launch_run(app, &state, &workspace, request)?;
    Ok(start_outcome(run_id, snapshot, outcome, saved_run_path))
}

#[tauri::command]
pub async fn start_command_run(
    app: AppHandle,
    state: State<'_, AppState>,
    root: String,
    project_names: Vec<String>,
    command: String,
    concurrency: u32,
) -> AppResult<RunStartOutcome> {
    ensure_no_active_run(&state)?;

    let line = command.trim().to_string();
    if line.is_empty() {
        return Err(AppError::new("Type a command first"));
    }

    let (workspace, config) = open_with_config(&root)?;
    let mut outcome = build_command_jobs(&workspace, &config, &project_names, &line);

    if outcome.jobs.is_empty() {
        return Err(AppError::new("Select at least one project"));
    }

    let jobs = std::mem::take(&mut outcome.jobs);
    let request = LaunchRequest {
        label: format!("cmd: {line}"),
        log_header: vec![
            format!("command:     {line}"),
            format!("projects:    {}", jobs.len()),
            format!("concurrency: {concurrency}"),
        ],
        jobs,
        options: RunOptions {
            concurrency: concurrency.max(1),
            mode: ExecutionMode::PerProject,
            dry_run: false,
        },
        backups: None,
    };

    let (run_id, snapshot) = launch_run(app, &state, &workspace, request)?;
    Ok(start_outcome(run_id, snapshot, outcome, None))
}

fn known_run(state: &AppState, run_id: &str) -> AppResult<Arc<RunContext>> {
    state
        .get(run_id)
        .ok_or_else(|| AppError::new("Unknown run"))
}

#[tauri::command]
pub fn abort_run(state: State<'_, AppState>, run_id: String) -> AppResult<()> {
    known_run(&state, &run_id)?.abort();
    Ok(())
}

#[tauri::command]
pub fn abort_job(state: State<'_, AppState>, run_id: String, job_name: String) -> AppResult<()> {
    known_run(&state, &run_id)?.abort_job(&job_name)
}

#[tauri::command]
pub fn restore_project(
    state: State<'_, AppState>,
    run_id: String,
    job_name: String,
) -> AppResult<Vec<String>> {
    known_run(&state, &run_id)?.restore_job_files(&job_name)
}

#[tauri::command]
pub fn get_run_snapshot(state: State<'_, AppState>, run_id: String) -> AppResult<RunSnapshot> {
    Ok(known_run(&state, &run_id)?.snapshot(true))
}

#[tauri::command]
pub fn render_run_report(summary: RunSummary, format: String) -> AppResult<String> {
    match format.as_str() {
        "markdown" => Ok(render_markdown(&summary)),
        "html" => Ok(render_html(&summary)),
        other => Err(AppError::new(format!("Unknown report format {other}"))),
    }
}
