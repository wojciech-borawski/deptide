mod common;

use common::TempDir;
use deptide_core::execution::lock_project;

#[test]
fn a_locked_project_is_refused_to_others_until_its_holder_lets_go() {
    let root = TempDir::new("lock-held");
    let locks = root.join("locks");
    let web = root.mkdir("web");

    let held = lock_project(&locks, &web, "first run").expect("the first lock");
    let refused = lock_project(&locks, &web, "second run")
        .err()
        .map(|error| error.to_string());
    let other_project = lock_project(&locks, &root.mkdir("api"), "second run");
    drop(held);
    let after_release = lock_project(&locks, &web, "third run");

    let refused = refused.expect("a second lock on the same project is refused");
    assert!(refused.contains("first run"), "{refused}");
    assert!(
        refused.contains(&std::process::id().to_string()),
        "{refused}"
    );
    assert!(other_project.is_ok());
    assert!(after_release.is_ok());
}

fn lock_file_of(locks: &std::path::Path, project: &std::path::Path) -> std::path::PathBuf {
    let held = lock_project(locks, project, "probe").expect("probe lock");
    let path = std::fs::read_dir(locks)
        .unwrap()
        .flatten()
        .next()
        .unwrap()
        .path();
    drop(held);
    path
}

fn exited_process_id() -> u32 {
    let mut child = if cfg!(windows) {
        std::process::Command::new("cmd")
            .args(["/C", "exit 0"])
            .spawn()
    } else {
        std::process::Command::new("true").spawn()
    }
    .expect("a short process starts");
    let pid = child.id();
    child.wait().expect("it exits");
    pid
}

#[test]
fn a_lock_left_by_a_process_that_is_gone_is_taken_over() {
    let root = TempDir::new("lock-stale");
    let locks = root.join("locks");
    let web = root.mkdir("web");
    let path = lock_file_of(&locks, &web);
    std::fs::write(
        &path,
        serde_json::json!({
            "id": "crashed",
            "pid": exited_process_id(),
            "owner": "crashed run",
            "since": "2026-10-22T12:00:00Z",
            "directory": web.to_string_lossy(),
        })
        .to_string(),
    )
    .unwrap();

    let taken = lock_project(&locks, &web, "next run")
        .map(|_| ())
        .map_err(|error| error.to_string());

    assert!(taken.is_ok(), "{taken:?}");
}

#[test]
fn a_lock_whose_process_number_now_belongs_to_another_process_is_taken_over() {
    let root = TempDir::new("lock-reused-pid");
    let locks = root.join("locks");
    let web = root.mkdir("web");
    let path = lock_file_of(&locks, &web);
    std::fs::write(
        &path,
        serde_json::json!({
            "id": "earlier",
            "pid": std::process::id(),
            "processStarted": 1,
            "owner": "a run from before a restart",
            "since": "2026-10-22T12:00:00Z",
            "directory": web.to_string_lossy(),
        })
        .to_string(),
    )
    .unwrap();

    let taken = lock_project(&locks, &web, "next run")
        .map(|_| ())
        .map_err(|error| error.to_string());

    assert!(taken.is_ok(), "{taken:?}");
}

#[test]
fn an_old_lock_file_without_a_readable_owner_is_taken_over() {
    let root = TempDir::new("lock-unreadable");
    let locks = root.join("locks");
    let web = root.mkdir("web");
    let path = lock_file_of(&locks, &web);
    let file = std::fs::File::create(&path).unwrap();
    file.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(3600))
        .unwrap();
    drop(file);

    let taken = lock_project(&locks, &web, "next run")
        .map(|_| ())
        .map_err(|error| error.to_string());

    assert!(taken.is_ok(), "{taken:?}");
}

#[cfg(windows)]
#[test]
fn while_a_lock_is_held_others_can_read_its_owner_but_not_remove_or_overwrite_it() {
    let root = TempDir::new("lock-sharing");
    let locks = root.join("locks");
    let web = root.mkdir("web");
    let path = lock_file_of(&locks, &web);
    let held = lock_project(&locks, &web, "the desktop run").expect("the lock");

    let owner = std::fs::read_to_string(&path);
    let removed = std::fs::remove_file(&path);
    let overwritten = std::fs::write(&path, "{}");
    drop(held);

    assert!(owner.is_ok_and(|text| text.contains("the desktop run")));
    assert!(removed.is_err(), "a held lock file was removed");
    assert!(overwritten.is_err(), "a held lock file was overwritten");
    assert!(!path.exists());
}

#[cfg(windows)]
fn icacls(folder: &std::path::Path, args: &[&str]) {
    let changed = std::process::Command::new("icacls")
        .arg(folder)
        .args(args)
        .output()
        .expect("icacls runs");
    assert!(changed.status.success(), "{changed:?}");
}

#[cfg(windows)]
#[test]
fn a_locks_folder_that_cannot_be_written_reports_its_own_error() {
    let root = TempDir::new("lock-unwritable");
    let locks = root.mkdir("locks");
    let web = root.mkdir("web");
    let user = std::env::var("USERNAME").expect("a user name");
    icacls(&locks, &["/deny", &format!("{user}:(W)")]);

    let refused = lock_project(&locks, &web, "next run")
        .err()
        .map(|error| error.to_string());
    icacls(&locks, &["/remove:d", &user]);

    let refused = refused.expect("an unwritable locks folder refuses the lock");
    assert!(refused.contains("(os error 5)"), "{refused}");
}

#[cfg(unix)]
#[test]
fn a_lock_file_naming_a_live_process_that_does_not_hold_it_is_taken_over() {
    let root = TempDir::new("lock-unheld");
    let locks = root.join("locks");
    let web = root.mkdir("web");
    let path = lock_file_of(&locks, &web);
    std::fs::write(
        &path,
        serde_json::json!({
            "id": "unheld",
            "pid": std::process::id(),
            "owner": "a run whose process number came back",
            "since": "2026-10-22T12:00:00Z",
            "directory": web.to_string_lossy(),
        })
        .to_string(),
    )
    .unwrap();

    let taken = lock_project(&locks, &web, "next run")
        .map(|_| ())
        .map_err(|error| error.to_string());

    assert!(taken.is_ok(), "{taken:?}");
}
