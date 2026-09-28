mod common;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use common::{GitRepo, TempDir};
use deptide_core::domain::{
    BumpWhen, ExecutionMode, Job, JobStatus, PackageSpec, StepName, VersionBump, VersionPolicy,
};
use deptide_core::execution::{execute_run, ProgressSink, RunContext, RunEvent, RunOptions};
use deptide_core::scan::{version_at_fork_point, ForkPointVersion};

#[derive(Default)]
struct Lines {
    seen: Mutex<Vec<String>>,
}

impl ProgressSink for Lines {
    fn emit(&self, event: RunEvent) {
        if let RunEvent::LogLine { line, .. } = event {
            self.seen.lock().unwrap().push(line);
        }
    }
}

/// main at 1.0.0, branch A from main bumps to 1.1.0, branch B from A adds an
/// unrelated commit. HEAD is on B.
fn stacked_branches(label: &str) -> GitRepo {
    let repo = GitRepo::new(label);
    repo.commit_version("", "1.0.0");
    repo.git(&["checkout", "-q", "-b", "A"]);
    repo.commit_version("", "1.1.0");
    repo.git(&["checkout", "-q", "-b", "B"]);
    repo.commit_other("readme");
    repo
}

fn add_remote(repo: &GitRepo) {
    let remote = repo.outside().join("remote.git");
    repo.git(&["init", "-q", "--bare", remote.to_str().unwrap()]);
    repo.git(&["remote", "add", "origin", remote.to_str().unwrap()]);
}

fn found(version: &str, base_branch: &str, commit: String) -> ForkPointVersion {
    ForkPointVersion::Found {
        version: version.to_string(),
        base_branch: base_branch.to_string(),
        commit,
    }
}

fn unavailable_reason(directory: &Path) -> String {
    match version_at_fork_point(directory) {
        ForkPointVersion::Unavailable(reason) => reason,
        other => panic!("expected no fork point, got {other:?}"),
    }
}

fn read_version(directory: &Path) -> String {
    let text = std::fs::read_to_string(directory.join("package.json")).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&text).unwrap();
    manifest["version"].as_str().unwrap().to_string()
}

async fn run_version_step(directory: PathBuf, when: BumpWhen) -> (JobStatus, Vec<String>) {
    run_version_job(directory, when, false).await
}

async fn run_version_job(
    directory: PathBuf,
    when: BumpWhen,
    dry_run: bool,
) -> (JobStatus, Vec<String>) {
    let job = Job {
        name: "web".to_string(),
        directory,
        packages: vec![PackageSpec::new("left-pad", "1.3.0")],
        steps: vec![StepName::Version],
        install_args: vec![],
        audit_fix_args: vec![],
        depends_on: vec![],
        command: None,
        version: VersionPolicy {
            bump: VersionBump::Minor,
            when,
        },
    };

    let sink = Arc::new(Lines::default());
    let context = RunContext::new(
        "run-version".to_string(),
        "version".to_string(),
        vec![job],
        RunOptions {
            concurrency: 1,
            mode: ExecutionMode::PerProject,
            dry_run,
        },
        sink.clone(),
        None,
    );

    let snapshot = execute_run(context).await;
    let lines = sink.seen.lock().unwrap().clone();
    (snapshot.jobs[0].status, lines)
}

#[test]
fn a_branch_stacked_on_a_bumped_branch_compares_with_that_branch() {
    let repo = stacked_branches("fork-stacked");

    assert_eq!(
        version_at_fork_point(repo.path()),
        found("1.1.0", "A", repo.short("A"))
    );
}

#[test]
fn a_branch_that_bumped_on_its_own_still_reports_the_version_it_started_from() {
    let repo = stacked_branches("fork-bumped");
    repo.commit_version("", "1.2.0");

    assert_eq!(
        version_at_fork_point(repo.path()),
        found("1.1.0", "A", repo.short("A"))
    );
}

#[test]
fn a_branch_without_own_commits_uses_the_branch_it_was_created_from() {
    let repo = GitRepo::new("fork-no-commits");
    repo.commit_version("", "1.0.0");
    repo.git(&["checkout", "-q", "-b", "A"]);
    repo.commit_version("", "1.1.0");
    repo.git(&["checkout", "-q", "-b", "B"]);

    assert_eq!(
        version_at_fork_point(repo.path()),
        found("1.1.0", "A", repo.short("HEAD"))
    );
}

#[test]
fn a_child_created_later_at_the_same_commit_is_not_the_base() {
    let repo = GitRepo::new("fork-child");
    repo.commit_version("", "1.0.0");
    repo.git(&["checkout", "-q", "-b", "A"]);
    repo.commit_version("", "1.1.0");
    repo.git(&["checkout", "-q", "-b", "B"]);
    repo.commit_version("", "1.2.0");
    repo.git(&["branch", "C"]);

    assert_eq!(
        version_at_fork_point(repo.path()),
        found("1.1.0", "A", repo.short("A")),
        "on B, the later child C must not count as its base"
    );

    repo.git(&["checkout", "-q", "C"]);
    assert_eq!(
        version_at_fork_point(repo.path()),
        found("1.2.0", "B", repo.short("B")),
        "on C, B was created first and is the base"
    );
}

#[test]
fn the_main_branch_has_no_base_to_compare_with() {
    let repo = stacked_branches("fork-main");
    repo.git(&["checkout", "-q", "main"]);

    assert!(unavailable_reason(repo.path()).contains("main branch"));
}

#[test]
fn a_folder_outside_git_has_no_fork_point() {
    let root = TempDir::new("fork-no-git");
    root.write("package.json", r#"{"name":"web","version":"1.0.0"}"#);

    assert!(unavailable_reason(root.path()).contains("not a git repository"));
}

#[test]
fn a_branch_with_no_other_branch_has_no_fork_point() {
    let repo = GitRepo::new("fork-alone");
    repo.git(&["checkout", "-q", "-b", "feature"]);
    repo.commit_version("", "1.0.0");

    assert!(unavailable_reason(repo.path()).contains("no other branch"));
}

#[test]
fn an_uncommitted_version_does_not_change_the_fork_point() {
    let repo = stacked_branches("fork-uncommitted");
    repo.write_version("", "1.2.0");

    assert_eq!(
        version_at_fork_point(repo.path()),
        found("1.1.0", "A", repo.short("A"))
    );
}

#[test]
fn a_project_nested_in_the_repository_reads_its_own_package_json() {
    let repo = GitRepo::new("fork-nested");
    repo.write_version("", "9.0.0");
    repo.commit_version("apps/web", "1.0.0");
    repo.git(&["checkout", "-q", "-b", "A"]);
    repo.commit_version("apps/web", "1.1.0");
    repo.git(&["checkout", "-q", "-b", "B"]);
    repo.commit_other("readme");

    assert_eq!(
        version_at_fork_point(&repo.path().join("apps/web")),
        found("1.1.0", "A", repo.short("A"))
    );
}

#[test]
fn the_upstream_of_the_branch_is_not_its_base() {
    let repo = stacked_branches("fork-upstream");
    repo.commit_version("", "1.2.0");
    add_remote(&repo);
    repo.git(&["push", "-q", "-u", "origin", "B"]);
    repo.commit_other("after-push");

    assert_eq!(
        version_at_fork_point(repo.path()),
        found("1.1.0", "A", repo.short("A"))
    );
}

#[test]
fn a_branch_built_on_top_of_the_current_one_is_not_its_base() {
    let repo = stacked_branches("fork-descendant");
    repo.git(&["checkout", "-q", "A"]);

    assert_eq!(
        version_at_fork_point(repo.path()),
        found("1.0.0", "main", repo.short("main"))
    );
}

#[test]
fn a_remote_branch_built_on_top_of_the_current_one_is_not_its_base() {
    let repo = GitRepo::new("fork-remote-descendant");
    repo.commit_version("", "1.0.0");
    repo.git(&["checkout", "-q", "-b", "A"]);
    repo.commit_version("", "1.1.0");
    repo.git(&["checkout", "-q", "-b", "B"]);
    repo.commit_version("", "1.2.0");
    repo.git(&["checkout", "-q", "-b", "C"]);
    repo.commit_other("child");
    add_remote(&repo);
    repo.git(&["push", "-q", "origin", "C"]);
    repo.git(&["checkout", "-q", "B"]);
    repo.git(&["branch", "-q", "-D", "C"]);

    assert_eq!(
        version_at_fork_point(repo.path()),
        found("1.1.0", "A", repo.short("A"))
    );
}

#[test]
fn a_detached_head_has_no_fork_point() {
    let repo = stacked_branches("fork-detached");
    repo.git(&["checkout", "-q", "--detach"]);
    repo.commit_version("", "1.2.0");

    assert_eq!(
        unavailable_reason(repo.path()),
        "HEAD is detached, check out a branch"
    );

    repo.git(&["checkout", "-q", "--detach", "main"]);
    assert_eq!(
        unavailable_reason(repo.path()),
        "HEAD is detached, check out a branch",
        "detached at the tip of main"
    );
}

#[test]
fn a_backup_made_after_the_branch_bumped_is_not_its_base() {
    let repo = GitRepo::new("fork-backup");
    repo.commit_version("", "1.0.0");
    repo.git(&["checkout", "-q", "-b", "A"]);
    repo.commit_version("", "1.1.0");
    repo.git(&["checkout", "-q", "-b", "B"]);
    repo.commit_version("", "1.2.0");
    repo.git(&["branch", "B-backup"]);
    repo.commit_other("readme");

    assert_eq!(
        version_at_fork_point(repo.path()),
        found("1.1.0", "A", repo.short("A"))
    );
}

#[test]
fn a_child_that_diverged_after_the_branch_bumped_is_not_its_base() {
    let repo = GitRepo::new("fork-diverged-child");
    repo.commit_version("", "1.0.0");
    repo.git(&["checkout", "-q", "-b", "A"]);
    repo.commit_version("", "1.1.0");
    repo.git(&["checkout", "-q", "-b", "B"]);
    repo.commit_version("", "1.2.0");
    repo.git(&["checkout", "-q", "-b", "C"]);
    repo.commit_other("child");
    repo.git(&["checkout", "-q", "B"]);
    repo.commit_other("readme");

    assert_eq!(
        version_at_fork_point(repo.path()),
        found("1.1.0", "A", repo.short("A")),
        "on B, the later child C must not count as its base"
    );

    repo.git(&["checkout", "-q", "C"]);
    assert_eq!(
        version_at_fork_point(repo.path()),
        found("1.2.0", "B", repo.short("B~1")),
        "on C, B was created first and is the base"
    );
}

#[test]
fn a_local_branch_without_a_reflog_counts_as_a_base_at_any_commit() {
    let repo = GitRepo::new("fork-other-no-reflog");
    repo.commit_version("", "1.0.0");
    repo.git(&["checkout", "-q", "-b", "A"]);
    repo.commit_version("", "1.1.0");
    repo.git(&["checkout", "-q", "-b", "B"]);
    repo.commit_version("", "1.2.0");
    repo.git(&["-c", "core.logAllRefUpdates=false", "branch", "B-backup"]);
    repo.commit_other("readme");

    assert_eq!(
        version_at_fork_point(repo.path()),
        found("1.2.0", "B-backup", repo.short("B-backup")),
        "B-backup was created later, but without its reflog nothing says so"
    );
}

#[test]
fn a_branch_without_a_reflog_has_no_fork_point() {
    let repo = stacked_branches("fork-own-no-reflog");
    repo.git(&[
        "reflog",
        "expire",
        "--expire=now",
        "--expire-unreachable=now",
        "refs/heads/B",
    ]);

    assert_eq!(
        unavailable_reason(repo.path()),
        "cannot tell when this branch was created, it has no reflog"
    );
}

#[test]
fn the_master_branch_has_no_base_to_compare_with() {
    let repo = stacked_branches("fork-master");
    repo.git(&["checkout", "-q", "main"]);
    repo.git(&["branch", "-m", "main", "master"]);

    assert_eq!(
        unavailable_reason(repo.path()),
        "on the main branch, no base branch to compare with"
    );
}

#[test]
fn a_repository_without_commits_has_no_fork_point() {
    let repo = GitRepo::new("fork-empty");
    repo.write_version("", "1.0.0");

    assert_eq!(
        unavailable_reason(repo.path()),
        "the repository has no commits yet"
    );
}

fn file_url(path: &Path) -> String {
    let path = path.to_str().unwrap().replace('\\', "/");
    if path.starts_with('/') {
        format!("file://{path}")
    } else {
        format!("file:///{path}")
    }
}

#[test]
fn a_shallow_clone_that_cuts_off_the_fork_point_says_so() {
    let repo = GitRepo::new("fork-shallow");
    repo.commit_version("", "1.0.0");
    repo.git(&["checkout", "-q", "-b", "A"]);
    repo.commit_version("", "1.1.0");
    repo.commit_other("readme");
    let clone = repo.outside().join("clone");
    let clone = clone.to_str().unwrap();
    repo.git(&[
        "clone",
        "-q",
        "--depth",
        "1",
        "--no-single-branch",
        &file_url(repo.path()),
        clone,
    ]);

    assert_eq!(
        repo.git(&["-C", clone, "rev-parse", "--abbrev-ref", "HEAD"]),
        "A"
    );
    assert_eq!(
        unavailable_reason(Path::new(clone)),
        "the clone is shallow, fetch the full history"
    );
}

#[test]
fn a_remote_branch_whose_name_ends_in_head_counts_as_a_base() {
    let repo = GitRepo::new("fork-remote-slash-head");
    repo.commit_version("", "1.0.0");
    repo.git(&["checkout", "-q", "-b", "feature/HEAD"]);
    repo.commit_version("", "1.1.0");
    add_remote(&repo);
    repo.git(&["push", "-q", "origin", "feature/HEAD"]);
    repo.git(&["checkout", "-q", "-b", "B"]);
    repo.git(&["branch", "-q", "-D", "feature/HEAD"]);
    repo.commit_other("readme");

    assert_eq!(
        version_at_fork_point(repo.path()),
        found(
            "1.1.0",
            "origin/feature/HEAD",
            repo.short("origin/feature/HEAD")
        )
    );
}

#[test]
fn a_branch_created_in_the_same_second_still_counts_as_a_base() {
    let repo = GitRepo::new("fork-same-second");
    repo.commit_version("", "1.0.0");
    repo.git(&["branch", "A"]);
    repo.repeat_last_second();
    repo.git(&["checkout", "-q", "-b", "B"]);

    assert_eq!(
        version_at_fork_point(repo.path()),
        found("1.0.0", "A", repo.short("HEAD")),
        "A and main both qualify and the shorter name is logged"
    );
}

#[test]
fn the_oldest_reflog_entry_dates_a_branch() {
    let repo = GitRepo::new("fork-reset");
    repo.commit_version("", "1.0.0");
    repo.git(&["checkout", "-q", "-b", "A"]);
    repo.commit_version("", "1.1.0");
    repo.git(&["checkout", "-q", "-b", "B"]);
    repo.commit_version("", "1.2.0");
    repo.git(&["branch", "C"]);
    repo.commit_other("scratch");
    repo.git(&["reset", "-q", "--hard", "HEAD~1"]);

    assert_eq!(
        version_at_fork_point(repo.path()),
        found("1.1.0", "A", repo.short("A")),
        "B was created before C even though B moved after C was created"
    );
}

#[test]
fn without_a_reflog_a_branch_at_the_same_commit_still_counts_as_a_base() {
    let repo = GitRepo::new("fork-no-reflog");
    repo.commit_version("", "1.0.0");
    repo.git(&["checkout", "-q", "-b", "A"]);
    repo.commit_version("", "1.1.0");
    repo.git(&["checkout", "-q", "-b", "B"]);
    repo.commit_version("", "1.2.0");
    repo.git(&["-c", "core.logAllRefUpdates=false", "branch", "C"]);

    assert_eq!(
        version_at_fork_point(repo.path()),
        found("1.2.0", "C", repo.short("HEAD")),
        "C was created later, but without its reflog nothing says so"
    );
}

#[test]
fn a_remote_branch_the_branch_tracks_under_another_name_is_its_base() {
    let repo = GitRepo::new("fork-remote-upstream");
    repo.commit_version("", "1.0.0");
    repo.git(&["checkout", "-q", "-b", "A"]);
    repo.commit_version("", "1.1.0");
    add_remote(&repo);
    repo.git(&["push", "-q", "origin", "A"]);
    repo.git(&["checkout", "-q", "main"]);
    repo.git(&["branch", "-q", "-D", "A"]);
    repo.git(&["checkout", "-q", "-b", "B", "--track", "origin/A"]);
    repo.commit_other("readme");

    assert_eq!(
        repo.git(&["rev-parse", "--symbolic-full-name", "@{u}"]),
        "refs/remotes/origin/A"
    );
    assert_eq!(
        version_at_fork_point(repo.path()),
        found("1.1.0", "origin/A", repo.short("origin/A"))
    );
}

#[test]
fn a_remote_branch_at_the_same_commit_counts_as_a_base_whenever_it_was_fetched() {
    let repo = GitRepo::new("fork-remote-same-commit");
    repo.commit_version("", "1.0.0");
    repo.git(&["checkout", "-q", "-b", "A"]);
    repo.commit_version("", "1.1.0");
    repo.git(&["checkout", "-q", "-b", "B"]);
    add_remote(&repo);
    repo.git(&["push", "-q", "origin", "A"]);
    repo.git(&["branch", "-q", "-D", "A"]);

    assert_eq!(
        version_at_fork_point(repo.path()),
        found("1.1.0", "origin/A", repo.short("HEAD"))
    );
}

#[test]
fn a_merged_side_branch_does_not_move_the_fork_point() {
    let repo = GitRepo::new("fork-merge");
    repo.commit_version("", "1.0.0");
    repo.git(&["checkout", "-q", "-b", "side"]);
    repo.commit_other("side");
    repo.git(&["checkout", "-q", "main"]);
    repo.git(&["checkout", "-q", "-b", "A"]);
    repo.commit_version("", "1.1.0");
    repo.git(&["checkout", "-q", "-b", "B"]);
    repo.commit_other("readme");
    repo.git(&[
        "merge",
        "-q",
        "--no-ff",
        "--no-edit",
        "-m",
        "merge side",
        "side",
    ]);
    repo.git(&["branch", "-q", "-D", "side"]);

    assert_eq!(
        version_at_fork_point(repo.path()),
        found("1.1.0", "A", repo.short("A"))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_version_step_bumps_when_the_branch_has_not_changed_the_version() {
    let repo = stacked_branches("fork-run-bump");

    let (status, lines) =
        run_version_step(repo.path().to_path_buf(), BumpWhen::NotBumpedOnBranch).await;

    let expected = format!(
        "version 1.1.0 is unchanged since A ({}), bumping",
        repo.short("A")
    );
    assert!(
        lines.contains(&expected),
        "expected {expected:?} in {lines:?}"
    );
    assert_eq!(status, JobStatus::Ok, "{lines:?}");
    assert_eq!(read_version(repo.path()), "1.2.0");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_version_step_skips_when_the_branch_already_changed_the_version() {
    let repo = stacked_branches("fork-run-skip");
    repo.commit_version("", "1.2.0");

    let (status, lines) =
        run_version_step(repo.path().to_path_buf(), BumpWhen::NotBumpedOnBranch).await;

    let expected = format!(
        "version 1.2.0 already changed on this branch (was 1.1.0 at A {}), bump skipped",
        repo.short("A")
    );
    assert!(
        lines.contains(&expected),
        "expected {expected:?} in {lines:?}"
    );
    assert_eq!(status, JobStatus::Ok);
    assert_eq!(read_version(repo.path()), "1.2.0");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_version_step_skips_an_uncommitted_version_change() {
    let repo = stacked_branches("fork-run-uncommitted");
    repo.write_version("", "1.2.0");

    let (status, lines) =
        run_version_step(repo.path().to_path_buf(), BumpWhen::NotBumpedOnBranch).await;

    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("version 1.2.0 already changed on this branch")),
        "{lines:?}"
    );
    assert_eq!(status, JobStatus::Ok);
    assert_eq!(read_version(repo.path()), "1.2.0");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_version_step_skips_when_the_fork_point_is_unknown() {
    let root = TempDir::new("fork-run-no-git");
    root.write("package.json", r#"{"name":"web","version":"1.0.0"}"#);

    let (status, lines) =
        run_version_step(root.path().to_path_buf(), BumpWhen::NotBumpedOnBranch).await;

    let expected = "cannot tell whether this branch bumped the version: the folder is not a git repository or git is not installed, bump skipped";
    assert!(
        lines.contains(&expected.to_string()),
        "expected {expected:?} in {lines:?}"
    );
    assert_eq!(status, JobStatus::Ok);
    assert_eq!(read_version(root.path()), "1.0.0");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_orphan_branch_skips_the_bump() {
    let repo = GitRepo::new("fork-run-orphan");
    repo.commit_version("", "1.0.0");
    repo.git(&["checkout", "-q", "--orphan", "other"]);
    repo.commit_version("", "1.0.0");

    let (status, lines) =
        run_version_step(repo.path().to_path_buf(), BumpWhen::NotBumpedOnBranch).await;

    let expected = "cannot tell whether this branch bumped the version: this branch shares no history with any other branch, bump skipped";
    assert!(
        lines.contains(&expected.to_string()),
        "expected {expected:?} in {lines:?}"
    );
    assert_eq!(status, JobStatus::Ok);
    assert_eq!(read_version(repo.path()), "1.0.0");
}

fn is_git_decision(line: &str) -> bool {
    line.ends_with(", bumping") || line.ends_with(", bump skipped")
}

#[tokio::test(flavor = "multi_thread")]
async fn the_version_step_always_bumps_without_looking_at_git() {
    let repo = GitRepo::new("fork-run-always");
    let project = repo
        .write_version("apps/web", "1.0.0")
        .parent()
        .unwrap()
        .to_path_buf();

    let (_, lookup_lines) = run_version_step(project.clone(), BumpWhen::NotBumpedOnBranch).await;
    assert!(
        lookup_lines.iter().any(|line| is_git_decision(line)),
        "the git lookup fails in this folder: {lookup_lines:?}"
    );

    let (status, lines) = run_version_step(project.clone(), BumpWhen::Always).await;

    assert!(!lines.iter().any(|line| is_git_decision(line)), "{lines:?}");
    assert_eq!(status, JobStatus::Ok, "{lines:?}");
    assert_eq!(read_version(&project), "1.1.0");
}

#[tokio::test(flavor = "multi_thread")]
async fn same_as_main_bumps_a_version_that_still_equals_main() {
    let repo = GitRepo::new("main-run-equal");
    repo.commit_version("", "1.0.0");
    repo.git(&["checkout", "-q", "-b", "A"]);
    repo.commit_other("readme");

    let (status, lines) = run_version_step(repo.path().to_path_buf(), BumpWhen::SameAsMain).await;

    assert!(
        lines.contains(&"version 1.0.0 still equals main, bumping".to_string()),
        "{lines:?}"
    );
    assert_eq!(status, JobStatus::Ok, "{lines:?}");
    assert_eq!(read_version(repo.path()), "1.1.0");
}

#[tokio::test(flavor = "multi_thread")]
async fn same_as_main_skips_a_version_that_differs_from_main() {
    let repo = stacked_branches("main-run-differs");

    let (status, lines) = run_version_step(repo.path().to_path_buf(), BumpWhen::SameAsMain).await;

    assert!(
        lines
            .contains(&"version 1.1.0 already differs from main (1.0.0), bump skipped".to_string()),
        "{lines:?}"
    );
    assert_eq!(status, JobStatus::Ok);
    assert_eq!(read_version(repo.path()), "1.1.0");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dry_run_names_the_bump_condition() {
    let root = TempDir::new("fork-dry-run");
    root.write("package.json", r#"{"name":"web","version":"1.0.0"}"#);

    for (when, expected) in [
        (BumpWhen::Always, "would bump the minor version"),
        (
            BumpWhen::SameAsMain,
            "would bump the minor version if it still equals the version on main",
        ),
        (
            BumpWhen::NotBumpedOnBranch,
            "would bump the minor version unless this branch already changed it",
        ),
    ] {
        let (_, lines) = run_version_job(root.path().to_path_buf(), when, true).await;
        assert!(
            lines.contains(&expected.to_string()),
            "expected {expected:?} in {lines:?}"
        );
    }
}

#[test]
fn a_project_added_on_this_branch_has_no_version_to_compare_with() {
    let repo = GitRepo::new("fork-new-project");
    repo.commit_other("readme");
    repo.git(&["checkout", "-q", "-b", "A"]);
    repo.commit_version("", "1.0.0");

    assert!(unavailable_reason(repo.path()).contains("package.json has no version"));
}

#[test]
fn the_remote_head_pointer_is_not_named_as_the_base() {
    let repo = GitRepo::new("fork-remote-head");
    repo.commit_version("", "1.0.0");
    add_remote(&repo);
    repo.git(&["push", "-q", "origin", "main"]);
    repo.git(&["remote", "set-head", "origin", "main"]);
    repo.git(&["checkout", "-q", "-b", "A"]);
    repo.commit_version("", "1.1.0");
    repo.git(&["branch", "-q", "-D", "main"]);

    assert_eq!(
        version_at_fork_point(repo.path()),
        found("1.0.0", "origin/main", repo.short("origin/main"))
    );
}
