use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use tauri::{AppHandle, State};

use crate::app::dto::RunStartOutcome;
use crate::app::events::TauriSink;
use crate::app::state::AppState;
use crate::app::tray;
use deptide_core::domain::{
    ExecutionMode, Job, PlanPreview, ProjectRestore, RunBackup, RunPlan, RunSnapshot, RunSummary,
    SavedRun, StepName,
};
use deptide_core::error::{AppError, AppResult};
use deptide_core::execution::{
    build_command_jobs, build_jobs, collect_package_specs, collect_steps, describe_version_policy,
    execute_run, list_run_backups as find_run_backups, nothing_to_do, prune_backups, render_html,
    render_markdown, restore_run, JobBuildOutcome, RestoreRequest, RunContext, RunFolders,
    RunLogger, RunOptions,
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

    let context = RunContext::with_folders(
        run_id.clone(),
        request.label,
        request.jobs,
        request.options,
        Arc::new(TauriSink::new(app)),
        Some(logger),
        RunFolders {
            backups: request.backups,
            locks: Some(workspace.locks_directory()),
        },
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

    if steps.contains(&StepName::Version) {
        header.push(format!("version:     {}", describe_version_policy(plan)));
    }

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
        version: plan.version,
        project_bumps: plan.project_bumps.clone(),
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
        return Err(AppError::new(nothing_to_do(&plan)));
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
pub async fn preview_plan(root: String, plan: RunPlan) -> AppResult<PlanPreview> {
    super::on_blocking_thread(move || {
        let (workspace, config) = open_with_config(&root)?;
        Ok(deptide_core::execution::preview_plan(
            &workspace, &config, &plan,
        ))
    })
    .await
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
pub async fn list_run_backups(root: String) -> AppResult<Vec<RunBackup>> {
    super::on_blocking_thread(move || {
        let workspace = Workspace::open(&root)?;
        Ok(find_run_backups(&workspace))
    })
    .await
}

#[tauri::command]
pub async fn restore_run_files(
    root: String,
    run: String,
    projects: Vec<String>,
) -> AppResult<Vec<ProjectRestore>> {
    super::on_blocking_thread(move || {
        let workspace = Workspace::open(&root)?;
        restore_run(&workspace, &RestoreRequest { run, projects })
    })
    .await
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

#[cfg(test)]
mod tests {
    use deptide_core::domain::{
        BumpWhen, ExecutionMode, RunPlan, StepName, VersionBump, VersionPolicy,
    };

    use deptide_core::workspace::{load_run, Workspace};

    use super::{save_plan_if_requested, update_log_header};

    fn plan(when: BumpWhen) -> RunPlan {
        RunPlan {
            project_names: vec!["web".to_string()],
            packages: vec![],
            steps: vec![StepName::Version],
            mode: ExecutionMode::PerProject,
            concurrency: 1,
            dry_run: false,
            extra_install_args: vec![],
            label: "bump".to_string(),
            save_as: None,
            version: VersionPolicy {
                bump: VersionBump::Minor,
                when,
            },
            project_bumps: [
                ("api".to_string(), VersionBump::Major),
                ("web".to_string(), VersionBump::Minor),
            ]
            .into(),
        }
    }

    fn version_line(when: BumpWhen) -> String {
        update_log_header(&plan(when), &[], &[StepName::Version], 1)
            .into_iter()
            .find(|line| line.starts_with("version:"))
            .expect("a version line")
    }

    #[test]
    fn the_log_header_names_the_bump_condition() {
        assert_eq!(
            version_line(BumpWhen::Always),
            "version:     minor, except api major"
        );
        assert_eq!(
            version_line(BumpWhen::SameAsMain),
            "version:     minor (only when still equal to main), except api major"
        );
        assert_eq!(
            version_line(BumpWhen::NotBumpedOnBranch),
            "version:     minor (only when not yet bumped on this branch), except api major"
        );
    }

    #[test]
    fn a_saved_run_keeps_the_per_project_bump_levels() {
        let root = std::env::temp_dir().join(format!("deptide-save-run-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let workspace = Workspace::open(&root).unwrap();
        let mut plan = plan(BumpWhen::NotBumpedOnBranch);
        plan.save_as = Some("bumps".to_string());

        save_plan_if_requested(&workspace, &plan).unwrap();
        let saved = load_run(&workspace, "bumps");
        let _ = std::fs::remove_dir_all(&root);

        let saved = saved.expect("the run was saved");
        assert_eq!(saved.version, plan.version);
        assert_eq!(saved.project_bumps, plan.project_bumps);
    }
}
