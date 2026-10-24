mod common;

use std::sync::Arc;

use common::{build_job, manifest, Lines, TempDir};
use deptide_core::domain::{
    BackedUpProject, ExecutionMode, Job, ProjectRestore, RestoreOutcome, RunSummary,
};
use deptide_core::execution::{
    execute_run, list_run_backups, lock_project, restore_run, RestoreRequest, RunContext,
    RunFolders, RunLogger, RunOptions, RunState,
};
use deptide_core::util::time::file_stamp;
use deptide_core::workspace::Workspace;

async fn finished_run(workspace: &Workspace, label: &str, jobs: Vec<Job>) {
    let logger = RunLogger::open(&workspace.logs_directory(), label, &[]).expect("logger");
    let context = RunContext::with_folders(
        "run".to_string(),
        label.to_string(),
        jobs,
        RunOptions {
            concurrency: 1,
            mode: ExecutionMode::PerProject,
            dry_run: false,
        },
        Arc::new(Lines::default()),
        Some(logger),
        RunFolders {
            backups: Some(workspace.backups_directory()),
            locks: Some(workspace.locks_directory()),
        },
    );
    execute_run(context).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_finished_run_lists_the_projects_it_backed_up() {
    let root = TempDir::new("restore-list");
    root.write("web/package.json", &manifest("web", "1.0.0", &[]));
    root.write("web/package-lock.json", "{\"name\":\"web\"}");
    root.write("api/package.json", &manifest("api", "1.0.0", &[]));
    let workspace = Workspace::open(root.path()).expect("workspace");

    finished_run(
        &workspace,
        "update libs",
        vec![
            build_job("web", root.join("web")),
            build_job("api", root.join("api")),
        ],
    )
    .await;
    let backups = list_run_backups(&workspace);

    assert_eq!(backups.len(), 1);
    assert!(
        backups[0].run.ends_with("-update-libs"),
        "{}",
        backups[0].run
    );
    assert_eq!(
        backups[0].projects,
        vec![
            BackedUpProject {
                name: "web".to_string(),
                directory: root.join("web").to_string_lossy().to_string(),
                files: vec!["package.json".to_string(), "package-lock.json".to_string()],
            },
            BackedUpProject {
                name: "api".to_string(),
                directory: root.join("api").to_string_lossy().to_string(),
                files: vec!["package.json".to_string()],
            },
        ]
    );
}

fn restored(name: &str, files: &[&str]) -> ProjectRestore {
    ProjectRestore {
        name: name.to_string(),
        outcome: RestoreOutcome::Restored {
            files: files.iter().map(|file| file.to_string()).collect(),
        },
    }
}

fn failed(name: &str, error: &str) -> ProjectRestore {
    ProjectRestore {
        name: name.to_string(),
        outcome: RestoreOutcome::Failed {
            error: error.to_string(),
        },
    }
}

fn held_since(workspace: &Workspace) -> String {
    let lock = std::fs::read_dir(workspace.locks_directory())
        .unwrap()
        .flatten()
        .next()
        .unwrap();
    let holder: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(lock.path()).unwrap()).unwrap();
    holder["since"].as_str().unwrap().to_string()
}

fn request(run: &str, projects: &[&str]) -> RestoreRequest {
    RestoreRequest {
        run: run.to_string(),
        projects: projects.iter().map(|project| project.to_string()).collect(),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn restoring_a_run_writes_back_only_the_backed_up_files() {
    let root = TempDir::new("restore-run");
    let web_manifest = manifest("web", "1.0.0", &[("left-pad", "1.2.0")]);
    root.write("web/package.json", &web_manifest);
    root.write("web/package-lock.json", "{\"name\":\"web\"}");
    let api_manifest = manifest("api", "1.0.0", &[]);
    root.write("api/package.json", &api_manifest);
    let workspace = Workspace::open(root.path()).expect("workspace");
    finished_run(
        &workspace,
        "update libs",
        vec![
            build_job("web", root.join("web")),
            build_job("api", root.join("api")),
        ],
    )
    .await;
    let run = list_run_backups(&workspace)[0].run.clone();

    root.write("web/package.json", "{\"changed\":true}");
    root.write("web/package-lock.json", "{\"changed\":true}");
    root.write("web/src/index.js", "changed after the run");
    root.write("api/package.json", "{\"changed\":true}");
    root.write("api/package-lock.json", "{\"created\":\"after the run\"}");
    let results = restore_run(&workspace, &request(&run, &[])).expect("restores");

    assert_eq!(
        results,
        vec![
            restored("web", &["package.json", "package-lock.json"]),
            restored("api", &["package.json"]),
        ]
    );
    let read = |relative: &str| std::fs::read_to_string(root.join(relative)).unwrap();
    assert_eq!(read("web/package.json"), web_manifest);
    assert_eq!(read("web/package-lock.json"), "{\"name\":\"web\"}");
    assert_eq!(read("web/src/index.js"), "changed after the run");
    assert_eq!(read("api/package.json"), api_manifest);
    assert_eq!(
        read("api/package-lock.json"),
        "{\"created\":\"after the run\"}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_restore_can_be_limited_to_named_projects() {
    let root = TempDir::new("restore-some");
    root.write("web/package.json", &manifest("web", "1.0.0", &[]));
    let api_manifest = manifest("api", "1.0.0", &[]);
    root.write("api/package.json", &api_manifest);
    let workspace = Workspace::open(root.path()).expect("workspace");
    finished_run(
        &workspace,
        "some",
        vec![
            build_job("web", root.join("web")),
            build_job("api", root.join("api")),
        ],
    )
    .await;
    let run = list_run_backups(&workspace)[0].run.clone();
    root.write("web/package.json", "{\"changed\":true}");
    root.write("api/package.json", "{\"changed\":true}");

    let results = restore_run(&workspace, &request(&run, &["api", "docs"])).expect("restores");

    assert_eq!(
        results,
        vec![
            restored("api", &["package.json"]),
            failed("docs", "This run holds no backup of docs"),
        ]
    );
    let read = |relative: &str| std::fs::read_to_string(root.join(relative)).unwrap();
    assert_eq!(read("api/package.json"), api_manifest);
    assert_eq!(read("web/package.json"), "{\"changed\":true}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_project_that_a_running_run_works_in_is_not_restored() {
    let root = TempDir::new("restore-busy");
    root.write("web/package.json", &manifest("web", "1.0.0", &[]));
    let api_manifest = manifest("api", "1.0.0", &[]);
    root.write("api/package.json", &api_manifest);
    let workspace = Workspace::open(root.path()).expect("workspace");
    finished_run(
        &workspace,
        "busy",
        vec![
            build_job("web", root.join("web")),
            build_job("api", root.join("api")),
        ],
    )
    .await;
    let run = list_run_backups(&workspace)[0].run.clone();
    root.write("web/package.json", "{\"running\":true}");
    root.write("api/package.json", "{\"changed\":true}");

    let _held = lock_project(
        &workspace.locks_directory(),
        &root.join("web"),
        "nightly run",
    )
    .expect("the run's lock");

    let results = restore_run(&workspace, &request(&run, &[])).expect("restores");

    assert_eq!(
        results,
        vec![
            failed(
                "web",
                &format!(
                    "Not restored: nightly run is working in this project (process {}, since {})",
                    std::process::id(),
                    held_since(&workspace)
                )
            ),
            restored("api", &["package.json"]),
        ]
    );
    let read = |relative: &str| std::fs::read_to_string(root.join(relative)).unwrap();
    assert_eq!(read("web/package.json"), "{\"running\":true}");
    assert_eq!(read("api/package.json"), api_manifest);
}

fn rewrite_summary(workspace: &Workspace, change: impl FnOnce(&mut serde_json::Value)) {
    let summary_file = std::fs::read_dir(workspace.logs_directory())
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .unwrap();
    let mut summary: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&summary_file).unwrap()).unwrap();
    change(&mut summary);
    std::fs::write(&summary_file, summary.to_string()).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_backup_folder_outside_the_backups_directory_is_ignored() {
    let root = TempDir::new("restore-escape");
    root.write("web/package.json", &manifest("web", "1.0.0", &[]));
    let workspace = Workspace::open(root.path()).expect("workspace");
    finished_run(
        &workspace,
        "escape",
        vec![build_job("web", root.join("web"))],
    )
    .await;
    rewrite_summary(&workspace, |summary| {
        summary["backupFolder"] = "../planted".into()
    });
    root.write("planted/web/package.json", "{\"planted\":true}");

    assert_eq!(list_run_backups(&workspace), vec![]);
}

#[tokio::test(flavor = "multi_thread")]
async fn projects_whose_names_share_a_backup_folder_are_not_offered() {
    let root = TempDir::new("restore-shared");
    root.write("one/package.json", &manifest("one", "1.0.0", &[]));
    root.write("two/package.json", &manifest("two", "1.0.0", &[]));
    root.write("api/package.json", &manifest("api", "1.0.0", &[]));
    let workspace = Workspace::open(root.path()).expect("workspace");
    finished_run(
        &workspace,
        "shared",
        vec![
            build_job("Web App", root.join("one")),
            build_job("web-app", root.join("two")),
            build_job("api", root.join("api")),
        ],
    )
    .await;

    let backups = list_run_backups(&workspace);
    let names: Vec<&str> = backups[0]
        .projects
        .iter()
        .map(|project| project.name.as_str())
        .collect();

    assert_eq!(names, ["api"]);
    let results = restore_run(&workspace, &request(&backups[0].run, &["Web App"])).unwrap();
    assert_eq!(
        results,
        vec![failed("Web App", "This run holds no backup of Web App")]
    );
}

#[test]
fn a_summary_from_before_the_backup_folder_was_recorded_finds_it_by_time_and_label() {
    let root = TempDir::new("restore-old-summary");
    let workspace = Workspace::open(root.path()).expect("workspace");
    let started_at = "2026-10-22T12:00:00.250+02:00";
    backed_up_web(&root, &legacy_folder(started_at), "old-run-original");
    save_summary(
        &workspace,
        "old-run",
        &past_summary(&workspace, "old-run", started_at, false),
    );

    let backups = list_run_backups(&workspace);

    assert_eq!(backups.len(), 1);
    assert_eq!(backups[0].folder, legacy_folder(started_at));
    assert_eq!(backups[0].projects[0].name, "web");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_whose_backups_were_pruned_has_nothing_to_restore() {
    let root = TempDir::new("restore-pruned");
    root.write("web/package.json", &manifest("web", "1.0.0", &[]));
    let workspace = Workspace::open(root.path()).expect("workspace");
    finished_run(
        &workspace,
        "pruned",
        vec![build_job("web", root.join("web"))],
    )
    .await;
    let backup = list_run_backups(&workspace).remove(0);

    std::fs::remove_dir_all(workspace.backups_directory().join(&backup.folder)).unwrap();

    assert_eq!(list_run_backups(&workspace), vec![]);
    let error = restore_run(&workspace, &request(&backup.run, &[])).unwrap_err();
    assert_eq!(
        error.message(),
        format!("Run {} has no backups left to restore", backup.run)
    );
}

fn real_run(workspace: &Workspace, label: &str, jobs: Vec<Job>) -> (String, Arc<RunContext>) {
    let logger = RunLogger::open(&workspace.logs_directory(), label, &[]).expect("logger");
    let run = deptide_core::execution::run_name(&logger.log_file().to_string_lossy());
    let context = RunContext::with_folders(
        run.clone(),
        label.to_string(),
        jobs,
        RunOptions {
            concurrency: 1,
            mode: ExecutionMode::PerProject,
            dry_run: false,
        },
        Arc::new(Lines::default()),
        Some(logger),
        RunFolders {
            backups: Some(workspace.backups_directory()),
            locks: Some(workspace.locks_directory()),
        },
    );
    (run, context)
}

#[tokio::test(flavor = "multi_thread")]
async fn two_runs_started_in_the_same_second_with_the_same_label_keep_their_own_backups() {
    let root = TempDir::new("restore-same-second");
    let first_manifest = manifest("web", "1.0.0", &[]);
    root.write("web/package.json", &first_manifest);
    let workspace = Workspace::open(root.path()).expect("workspace");
    let (first_run, first) = real_run(&workspace, "same", vec![build_job("web", root.join("web"))]);
    let (second_run, second) =
        real_run(&workspace, "same", vec![build_job("web", root.join("web"))]);

    execute_run(first).await;
    root.write("web/package.json", &manifest("web", "2.0.0", &[]));
    execute_run(second).await;
    root.write("web/package.json", &manifest("web", "3.0.0", &[]));
    let results = restore_run(&workspace, &request(&first_run, &[])).expect("restores");

    assert_eq!(results, vec![restored("web", &["package.json"])]);
    assert_eq!(
        std::fs::read_to_string(root.join("web/package.json")).unwrap(),
        first_manifest
    );
    assert_ne!(first_run, second_run);
    assert_eq!(list_run_backups(&workspace).len(), 2);
}

fn past_summary(workspace: &Workspace, run: &str, started_at: &str, dry_run: bool) -> RunSummary {
    let web = build_job("web", workspace.root().join("web"));
    let options = RunOptions {
        concurrency: 1,
        mode: ExecutionMode::PerProject,
        dry_run,
    };
    let log_file = workspace.logs_directory().join(format!("{run}.log"));
    let mut summary = RunState::new(run.to_string(), "same".to_string(), vec![web], &options)
        .finish(false, Some(log_file.to_string_lossy().to_string()));
    summary.started_at = started_at.to_string();
    summary
}

fn save_summary(workspace: &Workspace, run: &str, summary: &RunSummary) {
    std::fs::create_dir_all(workspace.logs_directory()).unwrap();
    std::fs::write(
        workspace.logs_directory().join(format!("{run}.json")),
        serde_json::to_string(summary).unwrap(),
    )
    .unwrap();
}

fn legacy_folder(started_at: &str) -> String {
    let moment = chrono::DateTime::parse_from_rfc3339(started_at).unwrap();
    format!("{}-same", file_stamp(moment.with_timezone(&chrono::Utc)))
}

fn backed_up_web(root: &TempDir, folder: &str, content: &str) {
    root.write(&format!("backups/{folder}/web/package.json"), content);
}

#[test]
fn an_old_summary_does_not_take_the_backup_of_a_newer_run_from_the_same_second() {
    let root = TempDir::new("restore-legacy-neighbour");
    root.write("web/package.json", "current");
    let workspace = Workspace::open(root.path()).expect("workspace");
    backed_up_web(
        &root,
        &legacy_folder("2026-10-22T12:00:00.000+02:00"),
        "old-run-original",
    );
    let newer_folder = legacy_folder("2026-10-22T12:00:01.500+02:00");
    backed_up_web(&root, &newer_folder, "newer-run-original");
    let old = past_summary(
        &workspace,
        "old-run",
        "2026-10-22T12:00:01.000+02:00",
        false,
    );
    save_summary(&workspace, "old-run", &old);
    let mut newer = past_summary(
        &workspace,
        "new-run",
        "2026-10-22T12:00:01.500+02:00",
        false,
    );
    newer.backup_folder = Some(newer_folder.clone());
    save_summary(&workspace, "new-run", &newer);

    let offered: Vec<String> = list_run_backups(&workspace)
        .into_iter()
        .map(|backup| backup.run)
        .collect();
    let error = restore_run(&workspace, &request("old-run", &[])).unwrap_err();

    assert_eq!(offered, vec!["new-run"]);
    assert_eq!(
        error.message(),
        "Run old-run has no backups left to restore"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("web/package.json")).unwrap(),
        "current"
    );
}

#[test]
fn a_dry_run_never_takes_a_backup_folder() {
    let root = TempDir::new("restore-legacy-dry");
    root.write("web/package.json", "current");
    let workspace = Workspace::open(root.path()).expect("workspace");
    let started_at = "2026-10-22T12:00:02.000+02:00";
    backed_up_web(&root, &legacy_folder(started_at), "someone-else");
    save_summary(
        &workspace,
        "dry-run",
        &past_summary(&workspace, "dry-run", started_at, true),
    );
    let mut recorded = past_summary(&workspace, "dry-recorded", started_at, true);
    recorded.backup_folder = Some(legacy_folder(started_at));
    save_summary(&workspace, "dry-recorded", &recorded);

    assert_eq!(list_run_backups(&workspace), vec![]);
    assert!(restore_run(&workspace, &request("dry-run", &[])).is_err());
    assert!(restore_run(&workspace, &request("dry-recorded", &[])).is_err());
    assert_eq!(
        std::fs::read_to_string(root.join("web/package.json")).unwrap(),
        "current"
    );
}

#[test]
fn two_old_summaries_matching_the_same_folder_get_no_restore() {
    let root = TempDir::new("restore-legacy-ambiguous");
    root.write("web/package.json", "current");
    let workspace = Workspace::open(root.path()).expect("workspace");
    backed_up_web(
        &root,
        &legacy_folder("2026-10-22T12:00:03.000+02:00"),
        "whose?",
    );
    for (run, started_at) in [
        ("first", "2026-10-22T12:00:03.100+02:00"),
        ("second", "2026-10-22T12:00:03.900+02:00"),
    ] {
        save_summary(
            &workspace,
            run,
            &past_summary(&workspace, run, started_at, false),
        );
    }

    assert_eq!(list_run_backups(&workspace), vec![]);
    assert!(restore_run(&workspace, &request("first", &[])).is_err());
    assert_eq!(
        std::fs::read_to_string(root.join("web/package.json")).unwrap(),
        "current"
    );
}
