use deptide_core::domain::{PackageSpec, RunPlan, Settings, StepName, UpdateConfig, VersionPolicy};
use deptide_core::error::{AppError, AppResult};
use deptide_core::execution::{
    build_jobs, collect_package_specs, collect_steps, prune_backups, RunOptions,
};
use deptide_core::workspace::{load_settings, open_with_config};

use super::{default_project_names, launch, Launch};
use crate::arguments::RunArgs;
use crate::exit_code::ExitCode;

fn parse_package(spec: &str) -> AppResult<PackageSpec> {
    let split = spec.rfind('@').filter(|index| *index > 0);
    match split {
        Some(index) => Ok(PackageSpec::new(&spec[..index], &spec[index + 1..])),
        None => Err(AppError::new(format!("Expected NAME@VERSION, got {spec}"))),
    }
}

fn log_header(
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
        format!("concurrency: {}", plan.concurrency),
        "source:      deptide-cli".to_string(),
    ];
    if plan.dry_run {
        header.push("mode:        dry run".to_string());
    }
    header
}

fn build_plan(args: &RunArgs, config: &UpdateConfig, settings: &Settings) -> AppResult<RunPlan> {
    let packages = args
        .packages
        .iter()
        .map(|spec| parse_package(spec))
        .collect::<AppResult<Vec<_>>>()?;

    Ok(RunPlan {
        project_names: default_project_names(&config.projects, &args.projects),
        packages,
        steps: args
            .steps
            .as_ref()
            .map(|steps| steps.iter().copied().map(StepName::from).collect())
            .unwrap_or_else(|| settings.steps.clone()),
        mode: args.mode.map(Into::into).unwrap_or(settings.mode),
        concurrency: args.concurrency.unwrap_or(settings.concurrency).max(1),
        dry_run: args.dry_run,
        extra_install_args: args.install_args.clone(),
        label: args.label.clone().unwrap_or_else(|| {
            args.packages
                .iter()
                .map(|spec| spec.split('@').next().unwrap_or(spec).to_string())
                .collect::<Vec<_>>()
                .join("+")
        }),
        save_as: None,
        version: VersionPolicy {
            bump: args.bump.into(),
            when: args.bump_when(),
        },
    })
}

pub async fn execute(args: RunArgs) -> AppResult<ExitCode> {
    let (workspace, config) = open_with_config(&args.workspace)?;
    let settings = load_settings(&workspace);
    let plan = build_plan(&args, &config, &settings)?;

    let outcome = build_jobs(&workspace, &config, &plan);
    for name in &outcome.missing {
        eprintln!("warning: {name} is not in the configuration, skipped");
    }
    for name in &outcome.without_packages {
        eprintln!("warning: {name} does not use any of the chosen packages, skipped");
    }
    if outcome.jobs.is_empty() {
        return Err(AppError::new(
            "Nothing to do: none of the selected projects uses the chosen packages",
        ));
    }

    let specs = collect_package_specs(&outcome.jobs);
    let steps = collect_steps(&outcome.jobs);
    let backups = (!plan.dry_run).then(|| workspace.backups_directory());
    if let Some(directory) = &backups {
        prune_backups(directory);
    }

    println!(
        "{} in {} projects, {} steps, {} at a time",
        specs.join(", "),
        outcome.jobs.len(),
        steps.len(),
        plan.concurrency
    );

    launch(
        &workspace,
        Launch {
            log_header: log_header(&plan, &specs, &steps, outcome.jobs.len()),
            label: plan.label,
            jobs: outcome.jobs,
            options: RunOptions {
                concurrency: plan.concurrency,
                mode: plan.mode,
                dry_run: plan.dry_run,
            },
            backups,
            show_output: !args.quiet,
            print_json: args.json,
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use deptide_core::domain::{BumpWhen, Settings, UpdateConfig, VersionBump, VersionPolicy};

    use super::build_plan;
    use crate::arguments::{Cli, Command};

    fn plan_version(flags: &[&str]) -> VersionPolicy {
        let mut args = vec!["deptide-cli", "run", "workspace", "-p", "left-pad@1.3.0"];
        args.extend_from_slice(flags);
        let Command::Run(run) = Cli::try_parse_from(args).unwrap().command else {
            panic!("expected the run command");
        };
        build_plan(&run, &UpdateConfig::default(), &Settings::default())
            .unwrap()
            .version
    }

    #[test]
    fn the_plan_carries_the_bump_and_the_bump_condition() {
        let cases: [(&[&str], VersionPolicy); 4] = [
            (
                &[],
                VersionPolicy {
                    bump: VersionBump::Patch,
                    when: BumpWhen::Always,
                },
            ),
            (
                &["--bump", "minor", "--bump-when", "not-bumped-on-branch"],
                VersionPolicy {
                    bump: VersionBump::Minor,
                    when: BumpWhen::NotBumpedOnBranch,
                },
            ),
            (
                &["--bump-when", "same-as-main"],
                VersionPolicy {
                    bump: VersionBump::Patch,
                    when: BumpWhen::SameAsMain,
                },
            ),
            (
                &["--bump", "major", "--bump-only-if-same-as-main"],
                VersionPolicy {
                    bump: VersionBump::Major,
                    when: BumpWhen::SameAsMain,
                },
            ),
        ];

        for (flags, expected) in cases {
            assert_eq!(plan_version(flags), expected, "{flags:?}");
        }
    }
}
