use deptide_core::domain::ExecutionMode;
use deptide_core::error::{AppError, AppResult};
use deptide_core::execution::{build_command_jobs, RunOptions};
use deptide_core::workspace::open_with_config;

use super::{default_project_names, launch, Launch};
use crate::arguments::ExecArgs;
use crate::exit_code::ExitCode;

pub async fn execute(args: ExecArgs) -> AppResult<ExitCode> {
    let line = args.command.join(" ").trim().to_string();
    if line.is_empty() {
        return Err(AppError::new("Type a command after the options"));
    }

    let (workspace, config) = open_with_config(&args.workspace)?;
    let names = default_project_names(&config.projects, &args.projects);
    let outcome = build_command_jobs(&workspace, &config, &names, &line);

    for name in &outcome.missing {
        eprintln!("warning: {name} is not in the configuration, skipped");
    }
    if outcome.jobs.is_empty() {
        return Err(AppError::new("Select at least one configured project"));
    }

    let concurrency = args.concurrency.max(1);
    println!(
        "$ {line}  ({} projects, {concurrency} at a time)",
        outcome.jobs.len()
    );

    launch(
        &workspace,
        Launch {
            label: format!("cmd: {line}"),
            log_header: vec![
                format!("command:     {line}"),
                format!("projects:    {}", outcome.jobs.len()),
                format!("concurrency: {concurrency}"),
                "source:      deptide-cli".to_string(),
            ],
            jobs: outcome.jobs,
            options: RunOptions {
                concurrency,
                mode: ExecutionMode::PerProject,
                dry_run: false,
            },
            backups: None,
            show_output: !args.quiet,
            print_json: args.json,
        },
    )
    .await
}
