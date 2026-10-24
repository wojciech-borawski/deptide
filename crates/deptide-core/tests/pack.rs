mod common;

use common::TempDir;
use deptide_core::domain::{
    ConfiguredProject, ExecutionMode, JobStatus, PackMode, PackageSpec, PlanPreview, RunPlan,
    StepName, UpdateConfig, VersionPolicy,
};
use std::sync::Arc;

use common::{read_version, Lines};
use deptide_core::execution::{
    build_jobs, collect_steps, execute_run, mark_packable_candidates, preview_plan,
    PackableLibraries, RunContext, RunFolders, RunOptions,
};
use deptide_core::workspace::{load_config, save_config, Workspace};

const LIBRARY_FOLDER: &str = "acme+core-91484758";

const LIBRARY_MANIFEST: &str =
    r#"{"name":"@acme/core","version":"1.2.3","main":"dist/index.js","scripts":{"build":"tsc"}}"#;

fn packed(mode: PackMode) -> PackageSpec {
    PackageSpec {
        pack: Some(mode),
        ..PackageSpec::new("@acme/core", "1.2.3")
    }
}

fn plan(names: &[&str], packages: Vec<PackageSpec>, mode: ExecutionMode) -> RunPlan {
    RunPlan {
        project_names: names.iter().map(|name| name.to_string()).collect(),
        packages,
        steps: vec![StepName::Uninstall, StepName::Install, StepName::Build],
        mode,
        concurrency: 2,
        dry_run: false,
        extra_install_args: vec![],
        label: "pack test".to_string(),
        save_as: None,
        version: VersionPolicy::default(),
        project_bumps: Default::default(),
    }
}

fn workspace_with_library() -> (TempDir, Workspace) {
    let root = TempDir::new("pack");
    let tool = root.mkdir("my tool");
    root.write("repos/my libs/core/package.json", LIBRARY_MANIFEST);
    root.write(
        "repos/web/package.json",
        r#"{"name":"web","version":"1.0.0","dependencies":{"@acme/core":"^1.0.0"}}"#,
    );
    root.write(
        "repos/api/package.json",
        r#"{"name":"api","version":"1.0.0","devDependencies":{"@acme/core":"^1.0.0"}}"#,
    );

    let workspace = Workspace::open(&tool).expect("workspace");
    let config = UpdateConfig {
        projects: vec![
            ConfiguredProject::new("core", "../repos/my libs/core"),
            ConfiguredProject::new("web", "../repos/web"),
            ConfiguredProject::new("api", "../repos/api"),
        ],
        install_args: vec!["--no-fund".to_string()],
        ..UpdateConfig::default()
    };
    save_config(&workspace, &config).expect("config saved");
    (root, workspace)
}

#[test]
fn a_packed_library_gets_a_pack_job_that_its_consumers_wait_for() {
    let (_root, workspace) = workspace_with_library();
    let config = load_config(&workspace).unwrap();

    let outcome = build_jobs(
        &workspace,
        &config,
        &plan(
            &["web", "api"],
            vec![packed(PackMode::NoSave)],
            ExecutionMode::PerProject,
        ),
    );

    let summary: Vec<(String, Vec<StepName>, Vec<String>)> = outcome
        .jobs
        .iter()
        .map(|job| (job.name.clone(), job.steps.clone(), job.depends_on.clone()))
        .collect();
    let steps = vec![StepName::Uninstall, StepName::Install, StepName::Build];
    assert_eq!(
        summary,
        vec![
            ("core".to_string(), vec![StepName::Pack], vec![]),
            ("web".to_string(), steps.clone(), vec!["core".to_string()]),
            ("api".to_string(), steps, vec!["core".to_string()]),
        ]
    );
}

fn job_steps(outcome: &deptide_core::execution::JobBuildOutcome) -> Vec<(String, Vec<StepName>)> {
    outcome
        .jobs
        .iter()
        .map(|job| (job.name.clone(), job.steps.clone()))
        .collect()
}

#[test]
fn a_selected_library_that_uses_none_of_the_packages_is_listed_and_only_packed() {
    let (_root, workspace) = workspace_with_library();
    let config = load_config(&workspace).unwrap();
    let plan = plan(
        &["core", "web"],
        vec![packed(PackMode::NoSave)],
        ExecutionMode::PerProject,
    );

    let outcome = build_jobs(&workspace, &config, &plan);
    let preview = preview_plan(&workspace, &config, &plan);

    assert_eq!(outcome.without_packages, vec!["core".to_string()]);
    assert_eq!(preview.without_packages, vec!["core".to_string()]);
    assert_eq!(
        job_steps(&outcome),
        vec![
            ("core".to_string(), vec![StepName::Pack]),
            (
                "web".to_string(),
                vec![StepName::Uninstall, StepName::Install, StepName::Build]
            ),
        ]
    );
    let (_, _, notes) = preview_steps(&preview, "core").remove(0);
    assert_eq!(
        notes,
        vec![
            "only packed: none of the chosen libraries applies to this project, so its other steps do not run"
                .to_string()
        ]
    );
}

#[test]
fn a_selected_library_packs_at_the_end_of_its_own_job() {
    let (root, workspace) = workspace_with_library();
    root.write(
        "repos/my libs/core/package.json",
        r#"{"name":"@acme/core","version":"1.2.3","scripts":{"build":"tsc"},"dependencies":{"left-pad":"^1.0.0"}}"#,
    );
    let config = load_config(&workspace).unwrap();

    let outcome = build_jobs(
        &workspace,
        &config,
        &plan(
            &["core", "web"],
            vec![
                PackageSpec::new("left-pad", "1.3.0"),
                packed(PackMode::NoSave),
            ],
            ExecutionMode::PerProject,
        ),
    );

    assert_eq!(
        job_steps(&outcome),
        vec![
            (
                "core".to_string(),
                vec![
                    StepName::Uninstall,
                    StepName::Install,
                    StepName::Build,
                    StepName::Pack
                ]
            ),
            (
                "web".to_string(),
                vec![StepName::Uninstall, StepName::Install, StepName::Build]
            ),
        ]
    );
}

#[test]
fn in_per_step_mode_the_pack_phase_comes_before_every_other_phase() {
    let (root, workspace) = workspace_with_library();
    root.write(
        "repos/other/package.json",
        r#"{"name":"other","version":"1.0.0","dependencies":{"left-pad":"^1.0.0"}}"#,
    );
    let mut config = load_config(&workspace).unwrap();
    config
        .projects
        .insert(0, ConfiguredProject::new("other", "../repos/other"));

    let outcome = build_jobs(
        &workspace,
        &config,
        &plan(
            &["other", "web"],
            vec![
                PackageSpec::new("left-pad", "1.3.0"),
                packed(PackMode::NoSave),
            ],
            ExecutionMode::PerStep,
        ),
    );

    assert_eq!(
        collect_steps(&outcome.jobs),
        vec![
            StepName::Pack,
            StepName::Uninstall,
            StepName::Install,
            StepName::Build
        ]
    );
}

fn preview_steps(preview: &PlanPreview, job: &str) -> Vec<(StepName, Vec<String>, Vec<String>)> {
    preview
        .jobs
        .iter()
        .find(|entry| entry.name == job)
        .unwrap_or_else(|| panic!("no job {job} in {preview:?}"))
        .steps
        .iter()
        .map(|step| {
            let notes = step.notes.iter().map(|note| note.text.clone()).collect();
            (step.step, step.commands.clone(), notes)
        })
        .collect()
}

fn library_pack_folder(preview: &PlanPreview, workspace: &Workspace, job: &str) -> String {
    let steps = preview_steps(preview, job);
    let (_, commands, _) = steps
        .iter()
        .find(|(step, _, _)| *step == StepName::Pack)
        .expect("a pack step");
    let command = commands.last().expect("npm pack");
    let folder = command
        .strip_prefix("npm pack --json --pack-destination \"")
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or_else(|| panic!("unexpected pack command {command}"));
    let packs = workspace.packs_directory().display().to_string();
    let run_folder = std::path::Path::new(folder)
        .parent()
        .unwrap()
        .display()
        .to_string();
    assert!(
        folder.starts_with(&packs)
            && run_folder.ends_with("-pack-test")
            && folder.ends_with(LIBRARY_FOLDER),
        "{folder} is not the library's folder in a run folder under {packs}"
    );
    folder.to_string()
}

#[test]
fn the_pack_job_previews_the_build_and_npm_pack_into_its_library_folder() {
    let (_root, workspace) = workspace_with_library();
    let config = load_config(&workspace).unwrap();

    let preview = preview_plan(
        &workspace,
        &config,
        &plan(
            &["web"],
            vec![packed(PackMode::NoSave)],
            ExecutionMode::PerProject,
        ),
    );

    let folder = library_pack_folder(&preview, &workspace, "core");
    assert_eq!(
        preview_steps(&preview, "core"),
        vec![(
            StepName::Pack,
            vec![
                "npm run build".to_string(),
                format!("npm pack --json --pack-destination \"{folder}\""),
            ],
            vec![],
        )]
    );
}

fn tarball(folder: &str) -> String {
    std::path::Path::new(folder)
        .join("acme-core-1.2.3.tgz")
        .display()
        .to_string()
}

#[test]
fn a_consumer_installs_the_tarball_without_saving_and_does_not_uninstall_it() {
    let (_root, workspace) = workspace_with_library();
    let config = load_config(&workspace).unwrap();

    let preview = preview_plan(
        &workspace,
        &config,
        &plan(
            &["web", "api"],
            vec![packed(PackMode::NoSave)],
            ExecutionMode::PerProject,
        ),
    );

    let folder = library_pack_folder(&preview, &workspace, "core");
    let install = vec![format!(
        "npm install \"{}\" --no-fund --no-save",
        tarball(&folder)
    )];
    let not_uninstalled = vec![
        "@acme/core not uninstalled: installed from a packed tarball without saving".to_string(),
    ];
    let no_build = vec!["no build script, step skipped".to_string()];
    for consumer in ["web", "api"] {
        assert_eq!(
            preview_steps(&preview, consumer),
            vec![
                (StepName::Uninstall, vec![], not_uninstalled.clone()),
                (StepName::Install, install.clone(), vec![]),
                (StepName::Build, vec![], no_build.clone()),
            ],
            "{consumer}"
        );
    }
}

#[test]
fn in_save_mode_a_consumer_uninstalls_and_installs_the_tarball_into_its_own_field() {
    let (_root, workspace) = workspace_with_library();
    let config = load_config(&workspace).unwrap();

    let preview = preview_plan(
        &workspace,
        &config,
        &plan(
            &["web", "api"],
            vec![packed(PackMode::Save)],
            ExecutionMode::PerProject,
        ),
    );

    let folder = library_pack_folder(&preview, &workspace, "core");
    let package_steps = |consumer: &str| -> Vec<Vec<String>> {
        preview_steps(&preview, consumer)
            .into_iter()
            .filter(|(step, _, _)| *step != StepName::Build)
            .map(|(_, commands, _)| commands)
            .collect()
    };
    assert_eq!(
        package_steps("web"),
        vec![
            vec!["npm uninstall @acme/core".to_string()],
            vec![format!("npm install \"{}\" --no-fund", tarball(&folder))],
        ]
    );
    assert_eq!(
        package_steps("api"),
        vec![
            vec!["npm uninstall @acme/core".to_string()],
            vec![format!(
                "npm install \"{}\" --save-dev --no-fund",
                tarball(&folder)
            )],
        ]
    );
}

fn selected_library_preview(mode: ExecutionMode) -> (TempDir, Workspace, PlanPreview) {
    let (root, workspace) = workspace_with_library();
    root.write(
        "repos/my libs/core/package.json",
        r#"{"name":"@acme/core","version":"1.2.3","scripts":{"build":"tsc"},"dependencies":{"left-pad":"^1.0.0"}}"#,
    );
    let config = load_config(&workspace).unwrap();
    let preview = preview_plan(
        &workspace,
        &config,
        &plan(
            &["core", "web"],
            vec![
                PackageSpec::new("left-pad", "1.3.0"),
                packed(PackMode::NoSave),
            ],
            mode,
        ),
    );
    (root, workspace, preview)
}

#[test]
fn a_library_built_by_its_own_build_step_is_packed_without_building_again() {
    let (_root, workspace, preview) = selected_library_preview(ExecutionMode::PerProject);

    let folder = library_pack_folder(&preview, &workspace, "core");
    let steps = preview_steps(&preview, "core");
    assert_eq!(
        steps.last(),
        Some(&(
            StepName::Pack,
            vec![format!("npm pack --json --pack-destination \"{folder}\"")],
            vec!["already built by its build step, packed without building again".to_string()],
        ))
    );
}

#[test]
fn in_per_step_mode_a_selected_library_packs_first_builds_for_it_and_says_so() {
    let (_root, workspace, preview) = selected_library_preview(ExecutionMode::PerStep);

    let folder = library_pack_folder(&preview, &workspace, "core");
    let steps = preview_steps(&preview, "core");
    assert_eq!(
        steps.first(),
        Some(&(
            StepName::Pack,
            vec![
                "npm run build".to_string(),
                format!("npm pack --json --pack-destination \"{folder}\"")
            ],
            vec![
                "packed before its own install phase, so the tarball does not hold this run's changes to it"
                    .to_string()
            ],
        ))
    );
}

#[test]
fn a_library_with_a_prepack_script_is_still_built_before_packing() {
    let (root, workspace) = workspace_with_library();
    root.write(
        "repos/my libs/core/package.json",
        r#"{"name":"@acme/core","version":"1.2.3","scripts":{"build":"tsc","prepack":"node -e \"console.log(1)\"","prepare":"husky"}}"#,
    );
    let config = load_config(&workspace).unwrap();

    let preview = preview_plan(
        &workspace,
        &config,
        &plan(
            &["web"],
            vec![packed(PackMode::NoSave)],
            ExecutionMode::PerProject,
        ),
    );

    let folder = library_pack_folder(&preview, &workspace, "core");
    assert_eq!(
        preview_steps(&preview, "core"),
        vec![(
            StepName::Pack,
            vec![
                "npm run build".to_string(),
                format!("npm pack --json --pack-destination \"{folder}\""),
            ],
            vec![],
        )]
    );
}

fn job_log(snapshot: &deptide_core::domain::RunSnapshot, job: &str) -> Vec<String> {
    snapshot
        .jobs
        .iter()
        .find(|entry| entry.name == job)
        .unwrap_or_else(|| panic!("no job {job}"))
        .log
        .clone()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dry_run_logs_the_pack_and_the_tarball_installs_without_running_them() {
    let (_root, workspace) = workspace_with_library();
    let config = load_config(&workspace).unwrap();
    let jobs = build_jobs(
        &workspace,
        &config,
        &plan(
            &["web"],
            vec![packed(PackMode::NoSave)],
            ExecutionMode::PerProject,
        ),
    )
    .jobs;
    let run_folder = jobs[0].packing.as_ref().unwrap().folder.clone();
    let folder = run_folder.join(LIBRARY_FOLDER);
    let context = RunContext::new(
        "run-pack".to_string(),
        "pack test".to_string(),
        jobs,
        RunOptions {
            concurrency: 2,
            mode: ExecutionMode::PerProject,
            dry_run: true,
        },
        Arc::new(Lines::default()),
        None,
    );

    execute_run(context.clone()).await;
    let snapshot = context.snapshot(true);

    let folder_text = folder.display().to_string();
    assert_eq!(
        job_log(&snapshot, "core"),
        vec![
            "would run npm run build".to_string(),
            format!("would run npm pack --json --pack-destination \"{folder_text}\""),
        ]
    );
    assert_eq!(
        job_log(&snapshot, "web"),
        vec![
            "@acme/core not uninstalled: installed from a packed tarball without saving"
                .to_string(),
            format!(
                "would run npm install \"{}\" --no-fund --no-save",
                tarball(&folder_text)
            ),
            "would run npm run build".to_string(),
        ]
    );
    assert!(!run_folder.exists(), "a dry run creates no pack folder");
}

const BUILDING_LIBRARY: &str = r#"{"name":"@acme/core","version":"1.2.3","main":"dist/index.js","files":["dist"],"scripts":{"build":"node -e \"require('fs').mkdirSync('dist',{recursive:true});require('fs').writeFileSync('dist/index.js','module.exports=1')\""}}"#;
const CONSUMER: &str =
    r#"{"name":"app","version":"1.0.0","private":true,"dependencies":{"@acme/core":"^1.0.0"}}"#;
const CONSUMER_LOCK: &str = r#"{"name":"app","version":"1.0.0","lockfileVersion":3,"requires":true,"packages":{"":{"name":"app","version":"1.0.0","dependencies":{"@acme/core":"^1.0.0"}}}}"#;

fn real_workspace() -> (TempDir, Workspace) {
    let root = TempDir::new("pack-real");
    let tool = root.mkdir("my tool");
    root.write("repos/my libs/core/package.json", BUILDING_LIBRARY);
    root.write("repos/my app/package.json", CONSUMER);
    root.write("repos/my app/package-lock.json", CONSUMER_LOCK);

    let workspace = Workspace::open(&tool).expect("workspace");
    let config = UpdateConfig {
        projects: vec![
            ConfiguredProject::new("core", "../repos/my libs/core"),
            ConfiguredProject::new("app", "../repos/my app"),
        ],
        install_args: vec!["--no-fund".to_string(), "--no-audit".to_string()],
        ..UpdateConfig::default()
    };
    save_config(&workspace, &config).expect("config saved");
    (root, workspace)
}

async fn real_run(
    workspace: &Workspace,
    mode: PackMode,
) -> (Arc<RunContext>, deptide_core::domain::RunSnapshot) {
    let config = load_config(workspace).unwrap();
    let mut plan = plan(
        &["app"],
        vec![PackageSpec {
            version: "9.9.9".to_string(),
            ..packed(mode)
        }],
        ExecutionMode::PerProject,
    );
    plan.steps = vec![StepName::Uninstall, StepName::Install];
    let jobs = build_jobs(workspace, &config, &plan).jobs;
    let context = RunContext::with_folders(
        "run-pack-real".to_string(),
        plan.label.clone(),
        jobs,
        RunOptions {
            concurrency: 2,
            mode: ExecutionMode::PerProject,
            dry_run: false,
        },
        Arc::new(Lines::default()),
        None,
        RunFolders {
            backups: Some(workspace.backups_directory()),
            locks: Some(workspace.locks_directory()),
        },
    );
    let snapshot = execute_run(context.clone()).await;
    (context, snapshot)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_no_save_run_installs_the_built_tarball_and_leaves_package_json_and_the_lockfile_alone() {
    let (root, workspace) = real_workspace();

    let (context, snapshot) = real_run(&workspace, PackMode::NoSave).await;

    let statuses: Vec<(String, JobStatus)> = snapshot
        .jobs
        .iter()
        .map(|job| (job.name.clone(), job.status))
        .collect();
    assert_eq!(
        statuses,
        vec![
            ("core".to_string(), JobStatus::Ok),
            ("app".to_string(), JobStatus::Ok)
        ],
        "{:#?}",
        context.snapshot(true).jobs
    );
    let installed = root.join("repos/my app/node_modules/@acme/core");
    assert_eq!(read_version(&installed), "1.2.3");
    assert!(
        installed.join("dist/index.js").is_file(),
        "the build ran before packing"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("repos/my app/package.json")).unwrap(),
        CONSUMER
    );
    assert_eq!(
        std::fs::read_to_string(root.join("repos/my app/package-lock.json")).unwrap(),
        CONSUMER_LOCK
    );
    let app = &snapshot.jobs[1];
    assert_eq!(app.installed[0].installed.as_deref(), Some("1.2.3"));
    assert_eq!(app.installed[0].expected, "1.2.3");
    assert!(app.installed[0].matches);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_save_run_points_package_json_at_the_tarball_and_restore_brings_it_back() {
    let (root, workspace) = real_workspace();

    let (context, snapshot) = real_run(&workspace, PackMode::Save).await;

    assert_eq!(
        snapshot.jobs[1].status,
        JobStatus::Ok,
        "{:#?}",
        context.snapshot(true).jobs
    );
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("repos/my app/package.json")).unwrap(),
    )
    .unwrap();
    let saved = manifest["dependencies"]["@acme/core"].as_str().unwrap();
    assert!(
        saved.starts_with("file:") && saved.ends_with("/acme-core-1.2.3.tgz"),
        "{saved}"
    );

    let restored = context.restore_job_files("app").expect("restored");

    assert_eq!(restored, vec!["package.json", "package-lock.json"]);
    assert_eq!(
        std::fs::read_to_string(root.join("repos/my app/package.json")).unwrap(),
        CONSUMER
    );
    assert_eq!(
        std::fs::read_to_string(root.join("repos/my app/package-lock.json")).unwrap(),
        CONSUMER_LOCK
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn consumers_are_skipped_when_the_pack_fails() {
    let (root, workspace) = real_workspace();
    root.write(
        "repos/my libs/core/package.json",
        r#"{"name":"@acme/core","version":"1.2.3","scripts":{"build":"node -e \"process.exit(3)\""}}"#,
    );

    let (_context, snapshot) = real_run(&workspace, PackMode::NoSave).await;

    let outcome: Vec<(String, JobStatus, String)> = snapshot
        .jobs
        .iter()
        .map(|job| (job.name.clone(), job.status, job.error.clone()))
        .collect();
    assert_eq!(
        outcome,
        vec![
            (
                "core".to_string(),
                JobStatus::Failed,
                "npm run build exited with code 3".to_string()
            ),
            (
                "app".to_string(),
                JobStatus::Skipped,
                "not run because core did not succeed".to_string()
            ),
        ]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_packed_library_that_is_not_a_configured_project_fails_the_install() {
    let (_root, workspace) = workspace_with_library();
    let mut config = load_config(&workspace).unwrap();
    config.projects.retain(|project| project.name != "core");
    let plan = plan(
        &["web"],
        vec![packed(PackMode::NoSave)],
        ExecutionMode::PerProject,
    );
    let context = RunContext::new(
        "run-pack-missing".to_string(),
        "pack test".to_string(),
        build_jobs(&workspace, &config, &plan).jobs,
        RunOptions {
            concurrency: 1,
            mode: ExecutionMode::PerProject,
            dry_run: true,
        },
        Arc::new(Lines::default()),
        None,
    );

    let snapshot = execute_run(context).await;

    let outcome: Vec<(String, JobStatus, String)> = snapshot
        .jobs
        .iter()
        .map(|job| (job.name.clone(), job.status, job.error.clone()))
        .collect();
    assert_eq!(
        outcome,
        vec![(
            "web".to_string(),
            JobStatus::Failed,
            "@acme/core cannot be packed: no configured project has that name in its package.json"
                .to_string()
        )]
    );
}

#[test]
fn the_preview_names_a_packed_library_that_is_not_a_configured_project() {
    let (_root, workspace) = workspace_with_library();
    let mut config = load_config(&workspace).unwrap();
    config.projects.retain(|project| project.name != "core");

    let preview = preview_plan(
        &workspace,
        &config,
        &plan(
            &["web"],
            vec![packed(PackMode::NoSave)],
            ExecutionMode::PerProject,
        ),
    );

    let install = preview_steps(&preview, "web")
        .into_iter()
        .find(|(step, _, _)| *step == StepName::Install)
        .unwrap();
    assert_eq!(
        install,
        (
            StepName::Install,
            vec![],
            vec![
                "@acme/core cannot be packed: no configured project has that name in its package.json"
                    .to_string()
            ]
        )
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_preview_shows_the_commands_the_run_executes() {
    let (_root, workspace) = real_workspace();
    let config = load_config(&workspace).unwrap();
    let mut plan = plan(
        &["app"],
        vec![packed(PackMode::Save)],
        ExecutionMode::PerProject,
    );
    plan.steps = vec![StepName::Uninstall, StepName::Install];
    let preview = preview_plan(&workspace, &config, &plan);
    let jobs = build_jobs(&workspace, &config, &plan).jobs;
    let run_folder = jobs[0]
        .packing
        .as_ref()
        .unwrap()
        .folder
        .join(LIBRARY_FOLDER)
        .display()
        .to_string();
    let preview_folder = library_pack_folder(&preview, &workspace, "core");
    let context = RunContext::new(
        "run-pack-preview".to_string(),
        plan.label.clone(),
        jobs,
        RunOptions {
            concurrency: 1,
            mode: ExecutionMode::PerProject,
            dry_run: false,
        },
        Arc::new(Lines::default()),
        None,
    );

    execute_run(context.clone()).await;
    let snapshot = context.snapshot(true);

    for job in ["core", "app"] {
        let previewed: Vec<String> = preview_steps(&preview, job)
            .into_iter()
            .flat_map(|(_, commands, _)| commands)
            .map(|command| command.replace(&preview_folder, &run_folder))
            .collect();
        let ran: Vec<String> = job_log(&snapshot, job)
            .into_iter()
            .filter_map(|line| line.strip_prefix("$ ").map(str::to_string))
            .collect();
        assert_eq!(previewed, ran, "{job}");
    }
}

fn candidate(name: &str, local_version: Option<&str>) -> deptide_core::domain::DependencyCandidate {
    deptide_core::domain::DependencyCandidate {
        name: name.to_string(),
        local_version: local_version.map(str::to_string),
        local_branch: None,
        branch_suffix: None,
        current_ranges: vec!["1.0.0".to_string()],
        used_by: vec!["web".to_string()],
        is_dev_dependency: false,
        sections: vec![],
        library_project: None,
        ambiguous_projects: vec![],
    }
}

#[test]
fn a_candidate_a_configured_project_provides_is_marked_packable_with_its_version() {
    let (_root, workspace) = workspace_with_library();
    let config = load_config(&workspace).unwrap();
    let mut candidates = vec![candidate("@acme/core", None), candidate("left-pad", None)];

    mark_packable_candidates(
        &mut candidates,
        &PackableLibraries::read(&workspace, &config),
    );

    let marked: Vec<(Option<String>, Option<String>)> = candidates
        .into_iter()
        .map(|entry| (entry.library_project, entry.local_version))
        .collect();
    assert_eq!(
        marked,
        vec![
            (Some("core".to_string()), Some("1.2.3".to_string())),
            (None, None)
        ]
    );
}

#[test]
fn a_consumer_waits_for_its_selected_library_once() {
    let (_root, workspace) = workspace_with_library();
    let config = load_config(&workspace).unwrap();

    let outcome = build_jobs(
        &workspace,
        &config,
        &plan(
            &["core", "web"],
            vec![packed(PackMode::NoSave)],
            ExecutionMode::PerProject,
        ),
    );

    let web = outcome.jobs.iter().find(|job| job.name == "web").unwrap();
    assert_eq!(web.depends_on, vec!["core".to_string()]);
}

#[test]
fn a_library_packed_with_only_its_own_project_selected_does_not_install_its_own_tarball() {
    let (_root, workspace) = workspace_with_library();
    let config = load_config(&workspace).unwrap();

    let outcome = build_jobs(
        &workspace,
        &config,
        &plan(
            &["core"],
            vec![packed(PackMode::NoSave)],
            ExecutionMode::PerProject,
        ),
    );

    let packages: Vec<(String, Vec<String>)> = outcome
        .jobs
        .iter()
        .map(|job| {
            let names = job.packages.iter().map(|spec| spec.name.clone()).collect();
            (job.name.clone(), names)
        })
        .collect();
    assert_eq!(packages, vec![("core".to_string(), vec![])]);
    assert_eq!(
        job_steps(&outcome),
        vec![("core".to_string(), vec![StepName::Pack])]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_that_only_packs_takes_no_backup() {
    let (_root, workspace) = real_workspace();

    let (context, snapshot) = real_run(&workspace, PackMode::Save).await;

    assert_eq!(snapshot.jobs[0].status, JobStatus::Ok);
    let run_backups = workspace.backups_directory().join(&context.backup_folder);
    let backed_up: Vec<String> = std::fs::read_dir(&run_backups)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(backed_up, vec!["app".to_string()]);
}

#[tokio::test(flavor = "multi_thread")]
async fn packing_keeps_twenty_pack_folders_counting_its_own() {
    let (_root, workspace) = real_workspace();
    let packs = workspace.packs_directory();
    for second in 0..25 {
        std::fs::create_dir_all(packs.join(format!("2020-01-01T00-00-{second:02}-old"))).unwrap();
    }

    let (context, snapshot) = real_run(&workspace, PackMode::NoSave).await;

    assert_eq!(
        snapshot.jobs[0].status,
        JobStatus::Ok,
        "{:#?}",
        context.snapshot(true).jobs
    );
    let mut left: Vec<String> = std::fs::read_dir(&packs)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    left.sort();
    assert_eq!(left.len(), 20, "{left:?}");
    assert_eq!(left[0], "2020-01-01T00-00-06-old");
    assert!(left[19].ends_with("-pack-test"), "{left:?}");
}

fn npm(directory: &std::path::Path, args: &[&str]) {
    let status = std::process::Command::new(deptide_core::execution::npm_program())
        .args(args)
        .current_dir(directory)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("npm runs");
    assert!(status.success(), "npm {args:?} in {}", directory.display());
}

const OFFLINE_NPMRC: &str =
    "offline=true\nregistry=http://127.0.0.1:9/\nfund=false\nupdate-notifier=false\n";

fn workspace_with_published_library() -> (TempDir, Workspace) {
    let root = TempDir::new("pack-audit");
    let tool = root.mkdir("my tool");
    root.write(
        "repos/published/package.json",
        r#"{"name":"@acme/core","version":"1.0.0","main":"index.js"}"#,
    );
    root.write("repos/published/index.js", r#"module.exports="published""#);
    npm(&root.join("repos/published"), &["pack"]);
    root.write(
        "repos/my libs/core/package.json",
        r#"{"name":"@acme/core","version":"1.2.3","main":"index.js","scripts":{"build":"node -e \"require('fs').writeFileSync('index.js','module.exports=\\\"local-build\\\"')\""}}"#,
    );
    root.write(
        "repos/my libs/core/index.js",
        r#"module.exports="local-source""#,
    );
    root.write(
        "repos/my app/package.json",
        r#"{"name":"app","version":"1.0.0","private":true,"dependencies":{"@acme/core":"file:../published/acme-core-1.0.0.tgz"},"scripts":{"build":"node -e \"require('fs').writeFileSync('seen.txt',String(require('@acme/core')))\""}}"#,
    );
    root.write("repos/my app/.npmrc", OFFLINE_NPMRC);
    npm(&root.join("repos/my app"), &["install"]);

    let workspace = Workspace::open(&tool).expect("workspace");
    let config = UpdateConfig {
        projects: vec![
            ConfiguredProject::new("core", "../repos/my libs/core"),
            ConfiguredProject::new("app", "../repos/my app"),
        ],
        install_args: vec!["--no-fund".to_string()],
        ..UpdateConfig::default()
    };
    save_config(&workspace, &config).expect("config saved");
    (root, workspace)
}

#[tokio::test(flavor = "multi_thread")]
async fn with_the_default_steps_the_consumer_builds_against_the_packed_library_after_audit_fix() {
    let (root, workspace) = workspace_with_published_library();
    let config = load_config(&workspace).unwrap();
    let mut plan = plan(
        &["app"],
        vec![packed(PackMode::NoSave)],
        ExecutionMode::PerProject,
    );
    plan.steps = StepName::default_steps().to_vec();
    let preview = preview_plan(&workspace, &config, &plan);
    let jobs = build_jobs(&workspace, &config, &plan).jobs;
    let run_folder = jobs[0]
        .packing
        .as_ref()
        .unwrap()
        .folder
        .join(LIBRARY_FOLDER)
        .display()
        .to_string();
    let preview_folder = library_pack_folder(&preview, &workspace, "core");
    let context = RunContext::new(
        "run-pack-audit".to_string(),
        plan.label.clone(),
        jobs,
        RunOptions {
            concurrency: 1,
            mode: ExecutionMode::PerProject,
            dry_run: false,
        },
        Arc::new(Lines::default()),
        None,
    );

    let snapshot = execute_run(context.clone()).await;

    assert_eq!(
        snapshot.jobs[1].status,
        JobStatus::Ok,
        "{:#?}",
        context.snapshot(true).jobs
    );
    assert_eq!(
        std::fs::read_to_string(root.join("repos/my app/seen.txt")).unwrap(),
        "local-build"
    );
    let previewed: Vec<String> = preview_steps(&preview, "app")
        .into_iter()
        .flat_map(|(_, commands, _)| commands)
        .map(|command| command.replace(&preview_folder, &run_folder))
        .collect();
    let ran: Vec<String> = job_log(&context.snapshot(true), "app")
        .into_iter()
        .filter_map(|line| line.strip_prefix("$ ").map(str::to_string))
        .collect();
    assert_eq!(previewed, ran);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_run_installs_the_tarball_npm_pack_reports() {
    let (root, workspace) = real_workspace();
    root.write(
        "repos/my libs/core/package.json",
        &BUILDING_LIBRARY.replace(r#""version":"1.2.3""#, r#""version":"v1.2.3+build.7""#),
    );

    let (context, snapshot) = real_run(&workspace, PackMode::NoSave).await;

    let statuses: Vec<JobStatus> = snapshot.jobs.iter().map(|job| job.status).collect();
    assert_eq!(
        statuses,
        vec![JobStatus::Ok, JobStatus::Ok],
        "{:#?}",
        context.snapshot(true).jobs
    );
    let log = job_log(&context.snapshot(true), "app");
    assert!(
        log.iter().any(|line| line.starts_with("$ npm install")
            && line.contains("acme-core-v1.2.3+build.7.tgz")),
        "{log:#?}"
    );
    assert_eq!(
        read_version(&root.join("repos/my app/node_modules/@acme/core")),
        "v1.2.3+build.7"
    );
}

#[test]
fn the_preview_names_the_tarball_the_way_npm_does() {
    let (root, workspace) = workspace_with_library();
    root.write(
        "repos/my libs/core/package.json",
        &LIBRARY_MANIFEST.replace(r#""version":"1.2.3""#, r#""version":"v1.2.3+build.7""#),
    );
    let config = load_config(&workspace).unwrap();

    let preview = preview_plan(
        &workspace,
        &config,
        &plan(
            &["web"],
            vec![packed(PackMode::Save)],
            ExecutionMode::PerProject,
        ),
    );

    let folder = library_pack_folder(&preview, &workspace, "core");
    let (_, install, _) = preview_steps(&preview, "web").remove(1);
    let tarball = std::path::Path::new(&folder).join("acme-core-v1.2.3+build.7.tgz");
    assert_eq!(
        install,
        vec![format!("npm install \"{}\" --no-fund", tarball.display())]
    );
}

#[test]
fn the_preview_names_the_tarball_of_a_library_the_run_bumps_before_packing() {
    let (root, workspace) = workspace_with_library();
    root.write(
        "repos/my libs/core/package.json",
        r#"{"name":"@acme/core","version":"1.2.3","scripts":{"build":"tsc"},"dependencies":{"left-pad":"^1.0.0"}}"#,
    );
    let config = load_config(&workspace).unwrap();
    let mut plan = plan(
        &["core", "web"],
        vec![
            PackageSpec::new("left-pad", "1.3.0"),
            packed(PackMode::Save),
        ],
        ExecutionMode::PerProject,
    );
    plan.steps = vec![StepName::Install, StepName::Version];

    let preview = preview_plan(&workspace, &config, &plan);

    let folder = library_pack_folder(&preview, &workspace, "core");
    let (_, install, _) = preview_steps(&preview, "web").remove(0);
    let tarball = std::path::Path::new(&folder).join("acme-core-1.2.4.tgz");
    assert_eq!(
        install,
        vec![format!("npm install \"{}\" --no-fund", tarball.display())]
    );
}

#[test]
fn a_library_two_projects_provide_is_not_packed_and_the_preview_says_why() {
    let (root, workspace) = workspace_with_library();
    root.write("repos/core copy/package.json", LIBRARY_MANIFEST);
    let mut config = load_config(&workspace).unwrap();
    config
        .projects
        .push(ConfiguredProject::new("core-copy", "../repos/core copy"));

    let plan = plan(
        &["web"],
        vec![packed(PackMode::NoSave)],
        ExecutionMode::PerProject,
    );
    let outcome = build_jobs(&workspace, &config, &plan);
    let preview = preview_plan(&workspace, &config, &plan);

    assert_eq!(
        job_steps(&outcome),
        vec![(
            "web".to_string(),
            vec![StepName::Uninstall, StepName::Install, StepName::Build]
        )]
    );
    let (_, commands, notes) = preview_steps(&preview, "web").remove(1);
    assert_eq!(
        (commands, notes),
        (
            vec![],
            vec![
                "@acme/core cannot be packed: more than one configured project has that name in its package.json (core, core-copy)"
                    .to_string()
            ]
        )
    );
}

#[test]
fn a_candidate_two_configured_projects_provide_is_not_packable_and_names_them() {
    let (root, workspace) = workspace_with_library();
    root.write("repos/core copy/package.json", LIBRARY_MANIFEST);
    let mut config = load_config(&workspace).unwrap();
    config
        .projects
        .push(ConfiguredProject::new("core-copy", "../repos/core copy"));
    let mut candidates = vec![candidate("@acme/core", None)];

    mark_packable_candidates(
        &mut candidates,
        &PackableLibraries::read(&workspace, &config),
    );

    let marked = candidates.remove(0);
    assert_eq!(
        (marked.library_project, marked.ambiguous_projects),
        (None, vec!["core".to_string(), "core-copy".to_string()])
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pack_job_takes_its_library_lock_so_a_held_library_skips_it_and_its_consumers() {
    let (root, workspace) = real_workspace();
    let library = root.join("repos/my libs/core");
    let _held = deptide_core::execution::lock_project(
        &workspace.locks_directory(),
        &library,
        "the desktop run",
    )
    .expect("held elsewhere");

    let (_context, snapshot) = real_run(&workspace, PackMode::NoSave).await;

    let outcome: Vec<(&str, JobStatus, &str)> = snapshot
        .jobs
        .iter()
        .map(|job| (job.name.as_str(), job.status, job.error.as_str()))
        .collect();
    assert_eq!((outcome[0].0, outcome[0].1), ("core", JobStatus::Skipped));
    assert!(
        outcome[0]
            .2
            .starts_with("not run because the desktop run is working in this project"),
        "{}",
        outcome[0].2
    );
    assert_eq!(
        outcome[1],
        (
            "app",
            JobStatus::Skipped,
            "not run because core did not succeed"
        )
    );
    assert!(!workspace.packs_directory().exists());
}

fn workspace_with_two_packed_libraries() -> (TempDir, Workspace) {
    let (root, workspace) = workspace_with_published_library();
    root.write(
        "repos/published helper/package.json",
        r#"{"name":"@acme/helper","version":"0.9.0","main":"index.js"}"#,
    );
    root.write(
        "repos/published helper/index.js",
        r#"module.exports="published""#,
    );
    npm(&root.join("repos/published helper"), &["pack"]);
    root.write(
        "repos/helper/package.json",
        r#"{"name":"@acme/helper","version":"1.0.0","main":"index.js"}"#,
    );
    root.write("repos/helper/index.js", r#"module.exports="helper""#);
    root.write(
        "repos/my libs/core/package.json",
        r#"{"name":"@acme/core","version":"1.2.3","main":"index.js","devDependencies":{"@acme/helper":"file:../../published helper/acme-helper-0.9.0.tgz"},"scripts":{"build":"node -e \"require('fs').writeFileSync('index.js','module.exports=\\\"local-build\\\"')\""}}"#,
    );
    root.write("repos/my libs/core/.npmrc", OFFLINE_NPMRC);
    npm(&root.join("repos/my libs/core"), &["install"]);

    let mut config = load_config(&workspace).unwrap();
    config
        .projects
        .insert(0, ConfiguredProject::new("helper", "../repos/helper"));
    save_config(&workspace, &config).expect("config saved");
    (root, workspace)
}

#[tokio::test(flavor = "multi_thread")]
async fn in_per_step_mode_a_consumer_checks_the_version_that_was_packed_not_the_bumped_one() {
    let (root, workspace) = workspace_with_two_packed_libraries();
    let config = load_config(&workspace).unwrap();
    let mut plan = plan(
        &["core", "app"],
        vec![
            PackageSpec {
                pack: Some(PackMode::NoSave),
                ..PackageSpec::new("@acme/helper", "1.0.0")
            },
            packed(PackMode::NoSave),
        ],
        ExecutionMode::PerStep,
    );
    plan.steps = vec![
        StepName::Install,
        StepName::Version,
        StepName::Audit,
        StepName::Build,
    ];
    let jobs = build_jobs(&workspace, &config, &plan).jobs;
    let context = RunContext::new(
        "run-pack-per-step".to_string(),
        plan.label.clone(),
        jobs,
        RunOptions {
            concurrency: 2,
            mode: ExecutionMode::PerStep,
            dry_run: false,
        },
        Arc::new(Lines::default()),
        None,
    );

    let snapshot = execute_run(context.clone()).await;

    assert_eq!(
        read_version(&root.join("repos/my libs/core")),
        "1.2.4",
        "{:#?}",
        context.snapshot(true).jobs
    );
    let app = snapshot.jobs.iter().find(|job| job.name == "app").unwrap();
    assert_eq!(
        (app.status, app.warning.as_str()),
        (JobStatus::Ok, ""),
        "{:#?}",
        context.snapshot(true).jobs
    );
    assert_eq!(
        read_version(&root.join("repos/my app/node_modules/@acme/core")),
        "1.2.3"
    );
    assert_eq!(app.installed[0].expected, "1.2.3");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_library_whose_build_changes_its_version_is_installed_and_checked_as_npm_packed_it() {
    let (root, workspace) = real_workspace();
    root.write(
        "repos/my libs/core/package.json",
        r#"{"name":"@acme/core","version":"1.2.3","main":"dist/index.js","files":["dist"],"scripts":{"build":"node -e \"const fs=require('fs');fs.mkdirSync('dist',{recursive:true});fs.writeFileSync('dist/index.js','module.exports=1');const manifest=JSON.parse(fs.readFileSync('package.json'));manifest.version='1.3.0';fs.writeFileSync('package.json',JSON.stringify(manifest))\""}}"#,
    );

    let (context, snapshot) = real_run(&workspace, PackMode::NoSave).await;

    let statuses: Vec<(JobStatus, &str)> = snapshot
        .jobs
        .iter()
        .map(|job| (job.status, job.warning.as_str()))
        .collect();
    assert_eq!(
        statuses,
        vec![(JobStatus::Ok, ""), (JobStatus::Ok, "")],
        "{:#?}",
        context.snapshot(true).jobs
    );
    let log = job_log(&context.snapshot(true), "app");
    assert!(
        log.iter()
            .any(|line| line.starts_with("$ npm install") && line.contains("acme-core-1.3.0.tgz")),
        "{log:#?}"
    );
    assert_eq!(
        read_version(&root.join("repos/my app/node_modules/@acme/core")),
        "1.3.0"
    );
    assert_eq!(snapshot.jobs[1].installed[0].expected, "1.3.0");
}

async fn run_with_a_build_that_renames_the_library_to(
    name: &str,
) -> Vec<(String, JobStatus, String)> {
    let (root, workspace) = real_workspace();
    root.write(
        "repos/my libs/core/package.json",
        &r#"{"name":"@acme/core","version":"1.2.3","main":"dist/index.js","files":["dist"],"scripts":{"build":"node -e \"const fs=require('fs');fs.mkdirSync('dist',{recursive:true});fs.writeFileSync('dist/index.js','module.exports=1');const manifest=JSON.parse(fs.readFileSync('package.json'));manifest.name='NEW_NAME';fs.writeFileSync('package.json',JSON.stringify(manifest))\""}}"#
            .replace("NEW_NAME", name),
    );

    let (_context, snapshot) = real_run(&workspace, PackMode::NoSave).await;

    snapshot
        .jobs
        .iter()
        .map(|job| (job.name.clone(), job.status, job.error.clone()))
        .collect()
}

fn failed_pack_of(name: &str) -> Vec<(String, JobStatus, String)> {
    vec![
        (
            "core".to_string(),
            JobStatus::Failed,
            format!("npm pack packed {name} instead of @acme/core"),
        ),
        (
            "app".to_string(),
            JobStatus::Skipped,
            "not run because core did not succeed".to_string(),
        ),
    ]
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pack_whose_tarball_is_not_of_the_library_fails_and_skips_its_consumers() {
    assert_eq!(
        run_with_a_build_that_renames_the_library_to("@acme/other").await,
        failed_pack_of("@acme/other")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pack_of_a_package_whose_name_extends_the_library_name_fails() {
    assert_eq!(
        run_with_a_build_that_renames_the_library_to("@acme/core-extra").await,
        failed_pack_of("@acme/core-extra")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pack_of_the_unscoped_package_with_the_same_file_name_fails() {
    assert_eq!(
        run_with_a_build_that_renames_the_library_to("acme-core").await,
        failed_pack_of("acme-core")
    );
}

fn workspace_with_libraries_packing_to_the_same_file_name() -> (TempDir, Workspace) {
    let root = TempDir::new("pack-same-name");
    let tool = root.mkdir("my tool");
    let mut dependencies = Vec::new();
    for (folder, name, value) in [
        ("scoped", "@acme/core", "scoped"),
        ("unscoped", "acme-core", "unscoped"),
    ] {
        root.write(
            &format!("repos/published {folder}/package.json"),
            &format!(r#"{{"name":"{name}","version":"1.0.0","main":"index.js"}}"#),
        );
        root.write(
            &format!("repos/published {folder}/index.js"),
            &format!(r#"module.exports="published {value}""#),
        );
        npm(&root.join(&format!("repos/published {folder}")), &["pack"]);
        dependencies.push(format!(
            r#""{name}":"file:../published {folder}/acme-core-1.0.0.tgz""#
        ));
        root.write(
            &format!("repos/{folder}/package.json"),
            &format!(r#"{{"name":"{name}","version":"1.2.3","main":"index.js"}}"#),
        );
        root.write(
            &format!("repos/{folder}/index.js"),
            &format!(r#"module.exports="{value}""#),
        );
    }
    root.write(
        "repos/my app/package.json",
        &format!(
            r#"{{"name":"app","version":"1.0.0","private":true,"dependencies":{{{}}},"scripts":{{"build":"node -e \"require('fs').writeFileSync('seen.txt',[require('@acme/core'),require('acme-core')].join(','))\""}}}}"#,
            dependencies.join(",")
        ),
    );
    root.write("repos/my app/.npmrc", OFFLINE_NPMRC);
    npm(&root.join("repos/my app"), &["install"]);

    let workspace = Workspace::open(&tool).expect("workspace");
    let config = UpdateConfig {
        projects: vec![
            ConfiguredProject::new("scoped", "../repos/scoped"),
            ConfiguredProject::new("unscoped", "../repos/unscoped"),
            ConfiguredProject::new("app", "../repos/my app"),
        ],
        install_args: vec!["--no-fund".to_string()],
        ..UpdateConfig::default()
    };
    save_config(&workspace, &config).expect("config saved");
    (root, workspace)
}

#[tokio::test(flavor = "multi_thread")]
async fn libraries_whose_tarballs_share_a_file_name_are_each_installed() {
    let (root, workspace) = workspace_with_libraries_packing_to_the_same_file_name();
    let config = load_config(&workspace).unwrap();
    let mut plan = plan(
        &["app"],
        vec![
            packed(PackMode::NoSave),
            PackageSpec {
                pack: Some(PackMode::NoSave),
                ..PackageSpec::new("acme-core", "1.2.3")
            },
        ],
        ExecutionMode::PerProject,
    );
    plan.steps = vec![StepName::Install, StepName::Build];
    plan.concurrency = 1;
    let jobs = build_jobs(&workspace, &config, &plan).jobs;
    let context = RunContext::new(
        "run-pack-same-name".to_string(),
        plan.label.clone(),
        jobs,
        RunOptions {
            concurrency: 1,
            mode: ExecutionMode::PerProject,
            dry_run: false,
        },
        Arc::new(Lines::default()),
        None,
    );

    let snapshot = execute_run(context.clone()).await;

    let statuses: Vec<(&str, JobStatus, &str)> = snapshot
        .jobs
        .iter()
        .map(|job| (job.name.as_str(), job.status, job.warning.as_str()))
        .collect();
    assert_eq!(
        statuses,
        vec![
            ("scoped", JobStatus::Ok, ""),
            ("unscoped", JobStatus::Ok, ""),
            ("app", JobStatus::Ok, ""),
        ],
        "{:#?}",
        context.snapshot(true).jobs
    );
    assert_eq!(
        std::fs::read_to_string(root.join("repos/my app/seen.txt")).unwrap(),
        "scoped,unscoped"
    );
    assert_eq!(
        read_version(&root.join("repos/my app/node_modules/@acme/core")),
        "1.2.3"
    );
    assert_eq!(
        read_version(&root.join("repos/my app/node_modules/acme-core")),
        "1.2.3"
    );
}

fn workspace_with_libraries_whose_names_differ_only_in_case() -> (TempDir, Workspace) {
    let root = TempDir::new("pack-case");
    let tool = root.mkdir("my tool");
    let mut projects = Vec::new();
    for (folder, name) in [("upper", "Acme-Core"), ("lower", "acme-core")] {
        root.write(
            &format!("repos/published {folder}/package.json"),
            &format!(r#"{{"name":"{name}","version":"1.0.0","main":"index.js"}}"#),
        );
        root.write(
            &format!("repos/published {folder}/index.js"),
            &format!(r#"module.exports="published {name}""#),
        );
        npm(&root.join(&format!("repos/published {folder}")), &["pack"]);
        root.write(
            &format!("repos/{folder}/package.json"),
            &format!(r#"{{"name":"{name}","version":"1.2.3","main":"index.js"}}"#),
        );
        root.write(
            &format!("repos/{folder}/index.js"),
            &format!(r#"module.exports="local {name}""#),
        );
        root.write(
            &format!("repos/app {folder}/package.json"),
            &format!(
                r#"{{"name":"app-{folder}","version":"1.0.0","private":true,"dependencies":{{"{name}":"file:../published {folder}/{name}-1.0.0.tgz"}},"scripts":{{"build":"node -e \"require('fs').writeFileSync('seen.txt',require('{name}'))\""}}}}"#
            ),
        );
        root.write(&format!("repos/app {folder}/.npmrc"), OFFLINE_NPMRC);
        npm(&root.join(&format!("repos/app {folder}")), &["install"]);
        projects.push(ConfiguredProject::new(folder, format!("../repos/{folder}")));
        projects.push(ConfiguredProject::new(
            format!("app {folder}"),
            format!("../repos/app {folder}"),
        ));
    }

    let workspace = Workspace::open(&tool).expect("workspace");
    let config = UpdateConfig {
        projects,
        install_args: vec!["--no-fund".to_string()],
        ..UpdateConfig::default()
    };
    save_config(&workspace, &config).expect("config saved");
    (root, workspace)
}

#[tokio::test(flavor = "multi_thread")]
async fn libraries_whose_names_differ_only_in_case_are_each_installed() {
    let (root, workspace) = workspace_with_libraries_whose_names_differ_only_in_case();
    let config = load_config(&workspace).unwrap();
    let mut plan = plan(
        &["app upper", "app lower"],
        ["Acme-Core", "acme-core"]
            .into_iter()
            .map(|name| PackageSpec {
                pack: Some(PackMode::NoSave),
                ..PackageSpec::new(name, "1.2.3")
            })
            .collect(),
        ExecutionMode::PerProject,
    );
    plan.steps = vec![StepName::Install, StepName::Build];
    plan.concurrency = 1;
    let jobs = build_jobs(&workspace, &config, &plan).jobs;
    let context = RunContext::new(
        "run-pack-case".to_string(),
        plan.label.clone(),
        jobs,
        RunOptions {
            concurrency: 1,
            mode: ExecutionMode::PerProject,
            dry_run: false,
        },
        Arc::new(Lines::default()),
        None,
    );

    let snapshot = execute_run(context.clone()).await;

    assert!(
        snapshot.jobs.iter().all(|job| job.status == JobStatus::Ok),
        "{:#?}",
        context.snapshot(true).jobs
    );
    let seen = |folder: &str| {
        std::fs::read_to_string(root.join(&format!("repos/app {folder}/seen.txt"))).unwrap()
    };
    assert_eq!(
        (seen("upper"), seen("lower")),
        ("local Acme-Core".to_string(), "local acme-core".to_string())
    );
}
