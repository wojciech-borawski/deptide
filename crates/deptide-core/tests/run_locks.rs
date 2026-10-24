mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use common::{build_job, is_alive, manifest, Lines, TempDir};
use deptide_core::domain::{ExecutionMode, Job, JobStatus, StepName, VersionPolicy};
use deptide_core::execution::{
    execute_run, lock_project, ProgressSink, RunContext, RunEvent, RunFolders, RunOptions,
};

fn command_job(name: &str, directory: PathBuf, line: &str, depends_on: &[&str]) -> Job {
    Job {
        name: name.to_string(),
        directory,
        packages: vec![],
        steps: vec![],
        install_args: vec![],
        audit_fix_args: vec![],
        depends_on: depends_on.iter().map(|name| name.to_string()).collect(),
        command: Some(line.to_string()),
        version: VersionPolicy::default(),
        packing: None,
    }
}

fn project(root: &TempDir, name: &str) -> PathBuf {
    root.write(
        &format!("{name}/package.json"),
        &manifest(name, "1.0.0", &[]),
    );
    root.join(name)
}

fn one_project_at_a_time(concurrency: u32) -> RunOptions {
    RunOptions {
        concurrency,
        mode: ExecutionMode::PerProject,
        dry_run: false,
    }
}

fn step_by_step() -> RunOptions {
    RunOptions {
        concurrency: 1,
        mode: ExecutionMode::PerStep,
        dry_run: false,
    }
}

fn locked_run(
    locks: &Path,
    jobs: Vec<Job>,
    options: RunOptions,
    sink: Arc<dyn ProgressSink>,
) -> Arc<RunContext> {
    RunContext::with_folders(
        "run".to_string(),
        "nightly".to_string(),
        jobs,
        options,
        sink,
        None,
        RunFolders {
            backups: None,
            locks: Some(locks.to_path_buf()),
        },
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_skips_a_project_another_process_works_in_and_the_projects_that_need_it() {
    let root = TempDir::new("run-locked-project");
    let locks = root.join("locks");
    let web = project(&root, "web");
    let _desktop_run = lock_project(&locks, &web, "the desktop run").expect("held elsewhere");
    let context = locked_run(
        &locks,
        vec![
            command_job("web", web, "echo web", &[]),
            command_job("api", project(&root, "api"), "echo api", &["web"]),
            command_job("docs", project(&root, "docs"), "echo docs", &[]),
        ],
        one_project_at_a_time(1),
        Arc::new(Lines::default()),
    );

    let snapshot = execute_run(context).await;
    let outcome: Vec<(&str, JobStatus, &str)> = snapshot
        .jobs
        .iter()
        .map(|job| (job.name.as_str(), job.status, job.error.as_str()))
        .collect();

    assert_eq!(
        outcome[1],
        (
            "api",
            JobStatus::Skipped,
            "not run because web did not succeed"
        )
    );
    assert_eq!(outcome[2], ("docs", JobStatus::Ok, ""));
    assert_eq!((outcome[0].0, outcome[0].1), ("web", JobStatus::Skipped));
    assert!(
        outcome[0]
            .2
            .starts_with("not run because the desktop run is working in this project (process"),
        "{}",
        outcome[0].2
    );
}

fn try_lock(locks: &Path, directory: &Path) -> Result<(), String> {
    lock_project(locks, directory, "probe")
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn status_of(context: &RunContext, name: &str) -> JobStatus {
    context
        .snapshot(false)
        .jobs
        .into_iter()
        .find(|job| job.name == name)
        .map(|job| job.status)
        .unwrap()
}

async fn becomes_true(mut check: impl FnMut() -> bool) -> bool {
    for _ in 0..1200 {
        if check() {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    false
}

const HOLD_UNTIL_STOPPED: &str = "node -e \"require('fs').writeFileSync('started', ''); const start = Date.now(); setInterval(() => { if (require('fs').existsSync('stop') || Date.now() - start > 60000) process.exit(0); }, 50)\"";

#[tokio::test(flavor = "multi_thread")]
async fn a_project_stays_locked_while_its_job_runs_and_is_released_when_the_job_is_stopped() {
    let root = TempDir::new("run-lock-release");
    let locks = root.join("locks");
    let web = project(&root, "web");
    let api = project(&root, "api");
    let context = locked_run(
        &locks,
        vec![
            command_job("web", web.clone(), HOLD_UNTIL_STOPPED, &[]),
            command_job("api", api.clone(), HOLD_UNTIL_STOPPED, &[]),
        ],
        one_project_at_a_time(2),
        Arc::new(Lines::default()),
    );
    let run = tokio::spawn(execute_run(context.clone()));

    assert!(becomes_true(|| web.join("started").exists() && api.join("started").exists()).await);
    let while_running = try_lock(&locks, &web);
    context.abort_job("web").unwrap();
    assert!(becomes_true(|| try_lock(&locks, &web).is_ok()).await);
    let api_after_web_stopped = (status_of(&context, "api"), try_lock(&locks, &api));
    std::fs::write(api.join("stop"), "").unwrap();
    run.await.unwrap();

    assert!(
        while_running
            .as_ref()
            .is_err_and(|reason| reason.starts_with("the run \"nightly\" in ")),
        "{while_running:?}"
    );
    assert_eq!(api_after_web_stopped.0, JobStatus::Running);
    assert!(api_after_web_stopped.1.is_err());
    assert!(try_lock(&locks, &api).is_ok());
}

struct PanicsOnLine {
    job: &'static str,
    containing: &'static str,
}

impl ProgressSink for PanicsOnLine {
    fn emit(&self, event: RunEvent) {
        if matches!(&event, RunEvent::LogLine { job, line, .. } if job == self.job && line.contains(self.containing))
        {
            panic!("{} broke in the middle", self.job);
        }
    }
}

fn panics_on_any_line_of(job: &'static str) -> Arc<PanicsOnLine> {
    Arc::new(PanicsOnLine {
        job,
        containing: "",
    })
}

fn panics_while_writing(job: &'static str) -> Arc<PanicsOnLine> {
    Arc::new(PanicsOnLine {
        job,
        containing: "writing",
    })
}

const WRITES_UNTIL_STOPPED: &str = "const fs = require('fs'); fs.writeFileSync('pid', String(process.pid)); let ticks = 0; setInterval(() => { if (fs.existsSync('stop') || ticks > 600) process.exit(0); fs.writeFileSync('heartbeat', String(++ticks)); if (ticks === 1) console.log('writing'); }, 25);";

fn writer_project(root: &TempDir, name: &str) -> PathBuf {
    writer_project_built_by(root, name, "node writer.cjs")
}

fn writer_project_built_by(root: &TempDir, name: &str, build: &str) -> PathBuf {
    root.write(
        &format!("{name}/package.json"),
        &format!("{{\"name\": \"{name}\", \"version\": \"1.0.0\", \"scripts\": {{\"build\": \"{build}\"}}}}"),
    );
    root.write(&format!("{name}/writer.cjs"), WRITES_UNTIL_STOPPED);
    root.join(name)
}

async fn still_writes(project: &Path) -> bool {
    let heartbeat = || std::fs::read_to_string(project.join("heartbeat")).unwrap_or_default();
    let before = heartbeat();
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    before != heartbeat()
}

#[derive(Debug, PartialEq)]
struct Leftovers {
    still_writes: bool,
    process_running: bool,
}

const NOTHING_LEFT: Leftovers = Leftovers {
    still_writes: false,
    process_running: false,
};

async fn leftovers_of(project: &Path) -> Leftovers {
    let still_writes = still_writes(project).await;
    let pid = std::fs::read_to_string(project.join("pid")).expect("the writer started");
    let leftovers = Leftovers {
        still_writes,
        process_running: is_alive(pid.parse().expect("a process number")),
    };
    std::fs::write(project.join("stop"), "").unwrap();
    leftovers
}

async fn panic_while_job_writes(
    root: &TempDir,
    job: Job,
    options: RunOptions,
) -> (bool, Leftovers) {
    let locks = root.join("locks");
    let web = job.directory.clone();
    let context = locked_run(&locks, vec![job], options, panics_while_writing("web"));
    let run = tokio::spawn(execute_run(context));

    assert!(becomes_true(|| web.join("pid").exists()).await);
    let released = becomes_true(|| try_lock(&locks, &web).is_ok()).await;
    let leftovers = leftovers_of(&web).await;
    run.await.unwrap();
    (released, leftovers)
}

async fn panic_while_command_writes(root: &TempDir, web: PathBuf, line: &str) -> (bool, Leftovers) {
    panic_while_job_writes(
        root,
        command_job("web", web, line, &[]),
        one_project_at_a_time(1),
    )
    .await
}

async fn abort_while_job_writes(
    root: &TempDir,
    job: Job,
    options: RunOptions,
) -> (bool, Leftovers) {
    let locks = root.join("locks");
    let web = job.directory.clone();
    let context = locked_run(&locks, vec![job], options, Arc::new(Lines::default()));
    let run = tokio::spawn(execute_run(context));

    assert!(becomes_true(|| web.join("pid").exists()).await);
    run.abort();
    let released = becomes_true(|| try_lock(&locks, &web).is_ok()).await;
    let leftovers = leftovers_of(&web).await;
    (released, leftovers)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_that_panics_one_project_at_a_time_ends_its_processes_before_letting_go_of_its_project(
) {
    let root = TempDir::new("run-lock-panic-writer");
    let web = writer_project(&root, "web");

    let ended = panic_while_command_writes(&root, web, "node writer.cjs").await;

    assert_eq!(ended, (true, NOTHING_LEFT));
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_job_that_panics_kills_a_command_child_that_ignores_a_polite_stop() {
    let root = TempDir::new("run-lock-panic-stubborn");
    let web = writer_project(&root, "web");
    root.write(
        "web/stubborn.cjs",
        &format!("process.on('SIGTERM', () => {{}}); {WRITES_UNTIL_STOPPED}"),
    );

    let ended = panic_while_command_writes(&root, web, "node stubborn.cjs; true").await;

    assert_eq!(ended, (true, NOTHING_LEFT));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_that_panics_step_by_step_ends_its_processes_before_letting_go_of_its_project() {
    let root = TempDir::new("run-lock-panic-writer-steps");
    let locks = root.join("locks");
    let web = writer_project(&root, "web");
    let context = locked_run(
        &locks,
        vec![build_job("web", web.clone())],
        step_by_step(),
        panics_while_writing("web"),
    );

    let snapshot = tokio::time::timeout(std::time::Duration::from_secs(30), execute_run(context))
        .await
        .expect("the run ends");
    let released = try_lock(&locks, &web);
    let leftovers = leftovers_of(&web).await;

    assert_eq!(snapshot.jobs[0].status, JobStatus::Failed);
    assert!(released.is_ok(), "{released:?}");
    assert_eq!(leftovers, NOTHING_LEFT);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_task_that_is_aborted_ends_its_processes_before_letting_go_of_its_projects() {
    let root = TempDir::new("run-lock-abort-writer");
    let web = writer_project(&root, "web");

    let ended = abort_while_job_writes(
        &root,
        command_job("web", web, "node writer.cjs", &[]),
        one_project_at_a_time(1),
    )
    .await;

    assert_eq!(ended, (true, NOTHING_LEFT));
}

#[cfg(windows)]
const LEAVES_THE_WRITER_BEHIND: &str = "start /b node writer.cjs";

#[cfg(windows)]
#[tokio::test(flavor = "multi_thread")]
async fn a_stopped_job_ends_a_writer_its_shell_left_behind_before_letting_go_of_its_project() {
    let root = TempDir::new("run-lock-stop-left-behind");
    let locks = root.join("locks");
    let web = writer_project(&root, "web");
    let context = locked_run(
        &locks,
        vec![command_job(
            "web",
            web.clone(),
            LEAVES_THE_WRITER_BEHIND,
            &[],
        )],
        one_project_at_a_time(1),
        Arc::new(Lines::default()),
    );
    let run = tokio::spawn(execute_run(context.clone()));

    assert!(becomes_true(|| web.join("pid").exists()).await);
    let stopped = std::time::Instant::now();
    context.abort_job("web").unwrap();
    let released = becomes_true(|| try_lock(&locks, &web).is_ok()).await;
    let took = stopped.elapsed();
    let leftovers = leftovers_of(&web).await;
    run.await.unwrap();

    assert_eq!((released, leftovers), (true, NOTHING_LEFT));
    assert!(took < std::time::Duration::from_secs(5), "{took:?}");
}

#[cfg(windows)]
#[tokio::test(flavor = "multi_thread")]
async fn a_job_that_panics_one_project_at_a_time_ends_a_writer_its_shell_left_behind() {
    let root = TempDir::new("run-lock-panic-left-behind");
    let web = writer_project(&root, "web");

    let ended = panic_while_command_writes(&root, web, LEAVES_THE_WRITER_BEHIND).await;

    assert_eq!(ended, (true, NOTHING_LEFT));
}

#[cfg(windows)]
#[tokio::test(flavor = "multi_thread")]
async fn a_job_that_panics_step_by_step_ends_a_writer_its_shell_left_behind() {
    let root = TempDir::new("run-lock-panic-left-behind-steps");
    let web = writer_project_built_by(&root, "web", LEAVES_THE_WRITER_BEHIND);

    let ended = panic_while_job_writes(&root, build_job("web", web), step_by_step()).await;

    assert_eq!(ended, (true, NOTHING_LEFT));
}

#[cfg(windows)]
#[tokio::test(flavor = "multi_thread")]
async fn an_aborted_run_task_one_project_at_a_time_ends_a_writer_its_shell_left_behind() {
    let root = TempDir::new("run-lock-abort-left-behind");
    let web = writer_project(&root, "web");

    let ended = abort_while_job_writes(
        &root,
        command_job("web", web, LEAVES_THE_WRITER_BEHIND, &[]),
        one_project_at_a_time(1),
    )
    .await;

    assert_eq!(ended, (true, NOTHING_LEFT));
}

#[cfg(windows)]
#[tokio::test(flavor = "multi_thread")]
async fn an_aborted_run_task_step_by_step_ends_a_writer_its_shell_left_behind() {
    let root = TempDir::new("run-lock-abort-left-behind-steps");
    let web = writer_project_built_by(&root, "web", LEAVES_THE_WRITER_BEHIND);

    let ended = abort_while_job_writes(&root, build_job("web", web), step_by_step()).await;

    assert_eq!(ended, (true, NOTHING_LEFT));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_that_panics_does_not_keep_its_project_locked() {
    let root = TempDir::new("run-lock-panic");
    let locks = root.join("locks");
    let web = project(&root, "web");
    let context = locked_run(
        &locks,
        vec![command_job("web", web.clone(), "echo web", &[])],
        one_project_at_a_time(1),
        panics_on_any_line_of("web"),
    );

    execute_run(context.clone()).await;

    assert!(try_lock(&locks, &web).is_ok());
    drop(context);
}

struct PanicsWhenSkipped(&'static str);

impl ProgressSink for PanicsWhenSkipped {
    fn emit(&self, event: RunEvent) {
        if matches!(&event, RunEvent::JobChanged { job, .. } if job.name == self.0 && job.status == JobStatus::Skipped)
        {
            panic!("{} could not be reported as skipped", self.0);
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_that_panics_while_reporting_a_project_held_elsewhere_lets_go_of_its_other_projects()
{
    let root = TempDir::new("run-lock-skip-panic");
    let locks = root.join("locks");
    let web = project(&root, "web");
    let api = project(&root, "api");
    let _desktop_run = lock_project(&locks, &web, "the desktop run").expect("held elsewhere");
    let context = locked_run(
        &locks,
        vec![
            command_job("web", web, "echo web", &[]),
            command_job("api", api.clone(), "echo api", &[]),
        ],
        one_project_at_a_time(1),
        Arc::new(PanicsWhenSkipped("web")),
    );

    let ended = tokio::spawn(execute_run(context.clone())).await;

    assert!(ended.is_err_and(|error| error.is_panic()));
    assert_eq!(try_lock(&locks, &api), Ok(()));
    drop(context);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_that_panics_lets_go_of_its_project_while_the_rest_of_the_run_goes_on() {
    let root = TempDir::new("run-lock-panic-midrun");
    let locks = root.join("locks");
    let web = writer_project(&root, "web");
    let api = project(&root, "api");
    let context = locked_run(
        &locks,
        vec![
            command_job("web", web.clone(), "node writer.cjs", &[]),
            command_job("api", api.clone(), HOLD_UNTIL_STOPPED, &[]),
        ],
        one_project_at_a_time(2),
        panics_while_writing("web"),
    );
    let run = tokio::spawn(execute_run(context.clone()));

    assert!(becomes_true(|| api.join("started").exists()).await);
    let web_free_while_api_runs = becomes_true(|| try_lock(&locks, &web).is_ok()).await;
    let web_leftovers = leftovers_of(&web).await;
    let api_meanwhile = status_of(&context, "api");
    std::fs::write(api.join("stop"), "").unwrap();
    run.await.unwrap();

    assert!(web_free_while_api_runs);
    assert_eq!(web_leftovers, NOTHING_LEFT);
    assert_eq!(api_meanwhile, JobStatus::Running);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_library_job_that_panics_skips_its_consumer_and_the_run_still_ends() {
    let root = TempDir::new("run-lock-panic-library");
    let locks = root.join("locks");
    let library = project(&root, "library");
    let app = project(&root, "app");
    let context = locked_run(
        &locks,
        vec![
            command_job("library", library.clone(), "echo library", &[]),
            command_job("app", app.clone(), "echo app", &["library"]),
        ],
        one_project_at_a_time(2),
        panics_on_any_line_of("library"),
    );

    let ended = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        execute_run(context.clone()),
    )
    .await;

    assert!(ended.is_ok(), "the run never ended");
    assert_eq!(status_of(&context, "library"), JobStatus::Failed);
    assert_eq!(status_of(&context, "app"), JobStatus::Skipped);
    assert!(try_lock(&locks, &library).is_ok());
    assert!(try_lock(&locks, &app).is_ok());
}

fn half_written_lock(locks: &Path, directory: &Path) {
    let earlier: Vec<PathBuf> = std::fs::read_dir(locks)
        .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
        .unwrap_or_default();
    let held = lock_project(locks, directory, "probe").expect("probe lock");
    let path = std::fs::read_dir(locks)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .find(|path| !earlier.contains(path))
        .unwrap();
    drop(held);
    std::fs::write(path, "").unwrap();
}

#[tokio::test]
async fn taking_the_project_locks_of_a_run_leaves_the_async_worker_free() {
    let root = TempDir::new("run-lock-off-worker");
    let locks = root.join("locks");
    let jobs: Vec<Job> = ["a", "b", "c", "d", "e"]
        .into_iter()
        .map(|name| {
            let directory = project(&root, name);
            half_written_lock(&locks, &directory);
            command_job(name, directory, "echo", &[])
        })
        .collect();
    let started = std::time::Instant::now();
    let ticker = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        started.elapsed()
    });

    execute_run(locked_run(
        &locks,
        jobs,
        one_project_at_a_time(1),
        Arc::new(Lines::default()),
    ))
    .await;
    let ticked_after = ticker.await.unwrap();

    assert!(
        ticked_after < std::time::Duration::from_millis(250),
        "{ticked_after:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn step_by_step_a_project_that_failed_keeps_its_own_error_when_its_library_fails_in_a_later_step(
) {
    let root = TempDir::new("run-own-failure");
    let locks = root.join("locks");
    root.write(
        "library/package.json",
        "{\"name\": \"library\", \"version\": \"1.0.0\", \"scripts\": {\"build\": \"exit 3\"}}",
    );
    let library = build_job("library", root.join("library"));
    let app = Job {
        steps: vec![StepName::Pack],
        depends_on: vec!["library".to_string()],
        ..build_job("app", project(&root, "app"))
    };
    let context = locked_run(
        &locks,
        vec![library, app],
        step_by_step(),
        Arc::new(Lines::default()),
    );

    let snapshot = execute_run(context).await;
    let outcome: Vec<(&str, JobStatus, &str)> = snapshot
        .jobs
        .iter()
        .map(|job| (job.name.as_str(), job.status, job.error.as_str()))
        .collect();

    assert_eq!(
        outcome,
        vec![
            (
                "library",
                JobStatus::Failed,
                "npm run build exited with code 3"
            ),
            (
                "app",
                JobStatus::Failed,
                "the run has no folder for packed libraries"
            ),
        ]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_with_a_library_its_consumer_and_two_entries_for_one_folder_runs_them_all() {
    let root = TempDir::new("run-lock-library");
    let locks = root.join("locks");
    let library = project(&root, "library");
    let context = locked_run(
        &locks,
        vec![
            command_job("library", library.clone(), "echo library", &[]),
            command_job("app", project(&root, "app"), "echo app", &["library"]),
            command_job("library again", library.clone(), "echo again", &[]),
        ],
        one_project_at_a_time(2),
        Arc::new(Lines::default()),
    );

    let snapshot = execute_run(context.clone()).await;
    let statuses: Vec<JobStatus> = snapshot.jobs.iter().map(|job| job.status).collect();

    assert_eq!(statuses, vec![JobStatus::Ok; 3]);
    assert!(try_lock(&locks, &library).is_ok());
}
