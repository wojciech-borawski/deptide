use deptide_core::domain::{
    PackMode, PackageSpec, PlanPreview, RunPlan, Settings, StepName, UpdateConfig, VersionPolicy,
    VersionPreview,
};
use deptide_core::error::{AppError, AppResult};
use deptide_core::execution::{
    build_jobs, collect_package_specs, collect_steps, describe_version_policy, nothing_to_do,
    preview_plan, prune_backups, PackableLibraries, RunOptions,
};
use deptide_core::workspace::{load_settings, open_with_config};

use super::{launch, select_project_names, Launch};
use crate::arguments::RunArgs;
use crate::exit_code::ExitCode;

fn parse_package(spec: &str) -> AppResult<PackageSpec> {
    let split = spec.rfind('@').filter(|index| *index > 0);
    match split {
        Some(index) => Ok(PackageSpec::new(&spec[..index], &spec[index + 1..])),
        None => Err(AppError::new(format!("Expected NAME@VERSION, got {spec}"))),
    }
}

fn packed_package(
    name: &str,
    mode: PackMode,
    packable: &PackableLibraries,
) -> AppResult<PackageSpec> {
    let library = packable.libraries.get(name).ok_or_else(|| {
        if packable.ambiguous.contains_key(name) {
            AppError::new(packable.refusal(name).text())
        } else {
            AppError::new(format!(
                "{name} is not the package name of a configured project, so it cannot be packed"
            ))
        }
    })?;
    Ok(PackageSpec {
        pack: Some(mode),
        ..PackageSpec::new(name, library.version.clone().unwrap_or_default())
    })
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
    if steps.contains(&StepName::Version) {
        header.push(format!("version:     {}", describe_version_policy(plan)));
    }
    if plan.dry_run {
        header.push("mode:        dry run".to_string());
    }
    header
}

fn describe_version(version: &VersionPreview) -> String {
    match version {
        VersionPreview::Bump {
            current,
            next,
            level,
            reason,
        } => {
            let reason = reason
                .as_ref()
                .map(|reason| format!(" ({})", reason.text))
                .unwrap_or_default();
            let current = current.as_deref().unwrap_or("none");
            format!("{current} -> {next} {}{reason}", level.label())
        }
        VersionPreview::Skip { reason, .. } => format!("skipped, {}", reason.text),
        VersionPreview::Fail { reason, .. } => format!("fails, {}", reason.text),
    }
}

fn preview_lines(preview: &PlanPreview) -> Vec<String> {
    let mut lines = Vec::new();

    for job in &preview.jobs {
        if job.depends_on.is_empty() {
            lines.push(job.name.clone());
        } else {
            lines.push(format!("{}, after {}", job.name, job.depends_on.join(", ")));
        }

        for step in &job.steps {
            let label = step.step.label();
            match (&job.version, step.step) {
                (Some(version), StepName::Version) => {
                    lines.push(format!("  {label}: {}", describe_version(version)));
                }
                _ => lines.extend(
                    step.notes
                        .iter()
                        .map(|note| format!("  {label}: {}", note.text)),
                ),
            }
            lines.extend(
                step.commands
                    .iter()
                    .map(|command| format!("  {label}: {command}")),
            );
        }
    }

    lines
}

fn warn_about_skipped(missing: &[String], without_packages: &[String]) {
    for name in missing {
        eprintln!("warning: {name} is not in the configuration, skipped");
    }
    for name in without_packages {
        eprintln!("warning: {name} does not use any of the chosen packages, skipped");
    }
}

fn build_plan(
    args: &RunArgs,
    config: &UpdateConfig,
    settings: &Settings,
    packable: &PackableLibraries,
) -> AppResult<RunPlan> {
    let mut packages = args
        .packages
        .iter()
        .map(|spec| parse_package(spec))
        .collect::<AppResult<Vec<_>>>()?;
    let mode = if args.pack_save {
        PackMode::Save
    } else {
        PackMode::NoSave
    };
    for name in &args.packs {
        packages.push(packed_package(name, mode, packable)?);
    }

    let steps: Vec<StepName> = args
        .steps
        .as_ref()
        .map(|steps| steps.iter().copied().map(StepName::from).collect())
        .unwrap_or_else(|| settings.steps.clone());
    let label = args.label.clone().unwrap_or_else(|| {
        if args.packages.is_empty() && args.packs.is_empty() {
            steps
                .iter()
                .filter(|step| !step.changes_packages())
                .map(|step| step.label())
                .collect::<Vec<_>>()
                .join("+")
        } else {
            args.packages
                .iter()
                .map(|spec| spec.split('@').next().unwrap_or(spec).to_string())
                .chain(args.packs.iter().cloned())
                .collect::<Vec<_>>()
                .join("+")
        }
    });

    Ok(RunPlan {
        project_names: select_project_names(config, &args.projects, &args.groups)?,
        packages,
        steps,
        mode: args.mode.map(Into::into).unwrap_or(settings.mode),
        concurrency: args.concurrency.unwrap_or(settings.concurrency).max(1),
        dry_run: args.dry_run,
        extra_install_args: args.install_args.clone(),
        label,
        save_as: None,
        version: VersionPolicy {
            bump: args.bump.into(),
            when: args.bump_when(),
        },
        project_bumps: args.project_bumps.iter().cloned().collect(),
    })
}

pub async fn execute(args: RunArgs) -> AppResult<ExitCode> {
    let (workspace, config) = open_with_config(&args.workspace)?;
    let settings = load_settings(&workspace);
    let plan = build_plan(
        &args,
        &config,
        &settings,
        &PackableLibraries::read(&workspace, &config),
    )?;

    if args.preview {
        let preview = preview_plan(&workspace, &config, &plan);
        warn_about_skipped(&preview.missing, &preview.without_packages);
        if let Some(message) = preview.nothing_to_do {
            return Err(AppError::new(message.text));
        }
        for line in preview_lines(&preview) {
            println!("{line}");
        }
        return Ok(ExitCode::Success);
    }

    let outcome = build_jobs(&workspace, &config, &plan);
    warn_about_skipped(&outcome.missing, &outcome.without_packages);
    if outcome.jobs.is_empty() {
        return Err(AppError::new(nothing_to_do(&plan)));
    }

    let specs = collect_package_specs(&outcome.jobs);
    let steps = collect_steps(&outcome.jobs);
    let backups = (!plan.dry_run).then(|| workspace.backups_directory());
    if let Some(directory) = &backups {
        prune_backups(directory);
    }

    let libraries = if specs.is_empty() {
        "no libraries".to_string()
    } else {
        specs.join(", ")
    };
    println!(
        "{} in {} projects, {} steps, {} at a time",
        libraries,
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
    use deptide_core::domain::{
        BumpWhen, ConfiguredProject, JobPreview, PlanPreview, ProjectGroup, Reason, RunPlan,
        Settings, StepName, StepPreview, UpdateConfig, VersionBump, VersionPolicy, VersionPreview,
    };

    use super::{build_plan, log_header, preview_lines};
    use crate::arguments::{Cli, Command};
    use deptide_core::domain::{PackMode, PackageSpec};
    use deptide_core::execution::{LibraryProject, PackableLibraries};

    fn plan_version(flags: &[&str]) -> VersionPolicy {
        let mut args = vec!["deptide-cli", "run", "workspace", "-p", "left-pad@1.3.0"];
        args.extend_from_slice(flags);
        let Command::Run(run) = Cli::try_parse_from(args).unwrap().command else {
            panic!("expected the run command");
        };
        build_plan(
            &run,
            &UpdateConfig::default(),
            &Settings::default(),
            &Default::default(),
        )
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

    fn plan_from(args: &[&str]) -> Result<RunPlan, String> {
        let mut all = vec!["deptide-cli", "run", "workspace"];
        all.extend_from_slice(args);
        let cli = Cli::try_parse_from(all).map_err(|error| error.to_string())?;
        let Command::Run(run) = cli.command else {
            panic!("expected the run command");
        };
        build_plan(
            &run,
            &UpdateConfig::default(),
            &Settings::default(),
            &Default::default(),
        )
        .map_err(|error| error.to_string())
    }

    #[test]
    fn a_project_can_get_its_own_bump_level() {
        let plan = plan_from(&[
            "-p",
            "left-pad@1.3.0",
            "--bump",
            "patch",
            "--project-bump",
            "api=major",
            "--project-bump",
            "web=Minor",
        ])
        .unwrap();

        assert_eq!(plan.version.bump, VersionBump::Patch);
        assert_eq!(
            plan.project_bumps,
            [
                ("api".to_string(), VersionBump::Major),
                ("web".to_string(), VersionBump::Minor),
            ]
            .into()
        );
        assert!(plan_from(&["--project-bump", "api"]).is_err());
        assert!(plan_from(&["--project-bump", "api=huge"]).is_err());
    }

    fn plan_in(config: &UpdateConfig, args: &[&str]) -> Result<RunPlan, String> {
        let mut all = vec!["deptide-cli", "run", "workspace"];
        all.extend_from_slice(args);
        let cli = Cli::try_parse_from(all).map_err(|error| error.to_string())?;
        let Command::Run(run) = cli.command else {
            panic!("expected the run command");
        };
        build_plan(&run, config, &Settings::default(), &Default::default())
            .map_err(|error| error.to_string())
    }

    #[test]
    fn a_group_adds_its_projects_to_the_named_ones() {
        let config = UpdateConfig {
            projects: ["web", "admin", "api", "docs"]
                .map(|name| ConfiguredProject::new(name, format!("../{name}")))
                .to_vec(),
            groups: vec![
                ProjectGroup {
                    name: "frontend".to_string(),
                    projects: vec!["web".to_string(), "admin".to_string()],
                },
                ProjectGroup {
                    name: "services".to_string(),
                    projects: vec!["api".to_string(), "web".to_string()],
                },
            ],
            ..UpdateConfig::default()
        };

        let plan = plan_in(
            &config,
            &[
                "--steps", "build", "-P", "docs", "--group", "frontend", "--group", "services",
            ],
        )
        .unwrap();

        assert_eq!(plan.project_names, ["docs", "web", "admin", "api"]);
    }

    #[test]
    fn an_unknown_group_is_a_usage_error_naming_the_known_groups() {
        let config = UpdateConfig {
            groups: vec![ProjectGroup {
                name: "frontend".to_string(),
                projects: vec!["web".to_string()],
            }],
            ..UpdateConfig::default()
        };

        let error = plan_in(&config, &["--steps", "build", "--group", "backend"]).unwrap_err();

        assert_eq!(error, "Unknown group backend; known groups: frontend");
    }

    #[test]
    fn the_transcript_header_names_the_bump_policy() {
        let plan = plan_from(&[
            "--steps",
            "version",
            "--bump",
            "minor",
            "--bump-when",
            "not-bumped-on-branch",
            "--project-bump",
            "api=major",
        ])
        .unwrap();

        let header = log_header(&plan, &[], &[StepName::Version], 2);

        assert!(
            header.contains(
                &"version:     minor (only when not yet bumped on this branch), except api major"
                    .to_string()
            ),
            "{header:?}"
        );
    }

    fn plan_packing(args: &[&str]) -> Result<RunPlan, String> {
        let mut all = vec!["deptide-cli", "run", "workspace"];
        all.extend_from_slice(args);
        let cli = Cli::try_parse_from(all).map_err(|error| error.to_string())?;
        let Command::Run(run) = cli.command else {
            panic!("expected the run command");
        };
        let packable = PackableLibraries {
            libraries: [(
                "@acme/core".to_string(),
                LibraryProject {
                    project: "core".to_string(),
                    directory: "core".into(),
                    version: Some("1.2.3".to_string()),
                },
            )]
            .into(),
            ambiguous: Default::default(),
        };
        build_plan(
            &run,
            &UpdateConfig::default(),
            &Settings::default(),
            &packable,
        )
        .map_err(|error| error.to_string())
    }

    #[test]
    fn pack_asks_for_a_configured_library_at_its_own_version() {
        let packed = |mode| PackageSpec {
            pack: Some(mode),
            ..PackageSpec::new("@acme/core", "1.2.3")
        };

        assert_eq!(
            plan_packing(&["--pack", "@acme/core"]).unwrap().packages,
            vec![packed(PackMode::NoSave)]
        );
        assert_eq!(
            plan_packing(&[
                "--pack",
                "@acme/core",
                "--pack-save",
                "-p",
                "left-pad@1.3.0"
            ])
            .unwrap()
            .packages,
            vec![
                PackageSpec::new("left-pad", "1.3.0"),
                packed(PackMode::Save)
            ]
        );
        assert_eq!(
            plan_packing(&["--pack", "left-pad"]).unwrap_err(),
            "left-pad is not the package name of a configured project, so it cannot be packed"
        );
        assert!(plan_packing(&["--pack-save"]).is_err());
    }

    #[test]
    fn pack_refuses_a_library_two_projects_provide() {
        let cli = Cli::try_parse_from(["deptide-cli", "run", "workspace", "--pack", "@acme/core"])
            .unwrap();
        let Command::Run(run) = cli.command else {
            panic!("expected the run command");
        };
        let packable = PackableLibraries {
            libraries: Default::default(),
            ambiguous: [(
                "@acme/core".to_string(),
                vec!["core".to_string(), "core-copy".to_string()],
            )]
            .into(),
        };

        let refused = build_plan(
            &run,
            &UpdateConfig::default(),
            &Settings::default(),
            &packable,
        )
        .map_err(|error| error.to_string());

        assert_eq!(
            refused.unwrap_err(),
            "@acme/core cannot be packed: more than one configured project has that name in its package.json (core, core-copy)"
        );
    }

    #[test]
    fn the_audit_report_step_and_the_preview_flag_are_accepted() {
        let mut all = vec!["deptide-cli", "run", "workspace"];
        all.extend_from_slice(&["--steps", "audit-report,build", "--preview"]);
        let cli = Cli::try_parse_from(all).expect("the arguments parse");
        let Command::Run(run) = cli.command else {
            panic!("expected the run command");
        };

        assert!(run.preview);
        let plan = build_plan(
            &run,
            &UpdateConfig::default(),
            &Settings::default(),
            &Default::default(),
        )
        .unwrap();
        assert_eq!(plan.steps, vec![StepName::AuditReport, StepName::Build]);
    }

    fn step(step: StepName, commands: &[&str], notes: &[Reason]) -> StepPreview {
        StepPreview {
            step,
            commands: commands.iter().map(|line| line.to_string()).collect(),
            notes: notes.iter().map(|reason| reason.clone().into()).collect(),
        }
    }

    fn still_equals_main() -> Reason {
        Reason::StillEqualsMain {
            version: Some("1.4.2".to_string()),
        }
    }

    fn differs_from_main() -> Reason {
        Reason::DiffersFromMain {
            version: Some("2.0.0".to_string()),
            main: "1.0.0".to_string(),
        }
    }

    #[test]
    fn the_preview_prints_each_project_with_its_commands_and_version() {
        let preview = PlanPreview {
            jobs: vec![
                JobPreview {
                    name: "web".to_string(),
                    depends_on: vec![],
                    steps: vec![
                        step(StepName::Uninstall, &["npm uninstall @acme/core"], &[]),
                        step(
                            StepName::Version,
                            &["npm version minor --no-git-tag-version"],
                            &[still_equals_main()],
                        ),
                        step(StepName::Build, &[], &[Reason::NoBuildScript {}]),
                    ],
                    version: Some(VersionPreview::Bump {
                        current: Some("1.4.2".to_string()),
                        next: "1.5.0".to_string(),
                        level: VersionBump::Minor,
                        reason: Some(still_equals_main().into()),
                    }),
                },
                JobPreview {
                    name: "api".to_string(),
                    depends_on: vec!["web".to_string()],
                    steps: vec![step(StepName::Version, &[], &[differs_from_main()])],
                    version: Some(VersionPreview::Skip {
                        current: Some("2.0.0".to_string()),
                        level: VersionBump::Minor,
                        reason: differs_from_main().into(),
                    }),
                },
            ],
            missing: vec![],
            without_packages: vec![],
            nothing_to_do: None,
        };

        assert_eq!(
            preview_lines(&preview),
            vec![
                "web",
                "  uninstall: npm uninstall @acme/core",
                "  version: 1.4.2 -> 1.5.0 minor (version 1.4.2 still equals main)",
                "  version: npm version minor --no-git-tag-version",
                "  build: no build script, step skipped",
                "api, after web",
                "  version: skipped, version 2.0.0 already differs from main (1.0.0)",
            ]
        );
    }

    #[test]
    fn a_run_without_packages_is_labelled_after_its_steps() {
        let plan = plan_from(&["--steps", "version,build"]).unwrap();

        assert!(plan.packages.is_empty());
        assert_eq!(plan.label, "version+build");
    }
}
