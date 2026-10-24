mod common;

use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, Once};
use std::time::{Duration, Instant};

use common::{is_alive, manifest, TempDir};
use deptide_core::dependencies::{
    fetch_latest_versions, find_installed_copies, read_dependency_drift, DependencyRequest,
    DependencyRequests, LatestLookup,
};
use deptide_core::domain::{ConfiguredProject, CopyScope, PackageSpec, UpdateConfig};
use deptide_core::workspace::Workspace;

const FAKE_NPM: &str = r#"
const fs = require("fs");
const path = require("path");
const name = process.argv[3];
const record = (kind) => fs.appendFileSync(path.join(__dirname, `${name}.events`), `${kind} ${process.pid}\n`);
record("start");
if (name.startsWith("hang-")) {
  setInterval(() => {}, 1000);
} else {
  setTimeout(() => {
    record("end");
    console.log(JSON.stringify("1.2.3"));
  }, 300);
}
"#;

struct FakeNpm {
    folder: PathBuf,
}

static FAKE_NPM_USERS: Mutex<usize> = Mutex::new(0);

fn fake_npm() -> FakeNpm {
    let mut users = FAKE_NPM_USERS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let folder = std::env::temp_dir().join(format!("deptide-fake-npm-{}", std::process::id()));
    if *users == 0 {
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("fake-npm.cjs"), FAKE_NPM).unwrap();
        fs::write(
            folder.join("npm.cmd"),
            "@echo off\r\nnode \"%~dp0fake-npm.cjs\" %*\r\n",
        )
        .unwrap();
        let script = folder.join("npm");
        fs::write(
            &script,
            "#!/bin/sh\nexec node \"$(dirname \"$0\")/fake-npm.cjs\" \"$@\"\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    *users += 1;
    static ON_PATH: Once = Once::new();
    ON_PATH.call_once(|| {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let mut folders = vec![folder.clone()];
        folders.extend(std::env::split_paths(&path));
        std::env::set_var("PATH", std::env::join_paths(folders).unwrap());
    });
    FakeNpm { folder }
}

impl FakeNpm {
    fn started_pids(&self, name: &str) -> Vec<u32> {
        fs::read_to_string(self.folder.join(format!("{name}.events")))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| line.strip_prefix("start "))
            .map(|pid| pid.trim().parse().unwrap())
            .collect()
    }
}

impl Drop for FakeNpm {
    fn drop(&mut self) {
        let mut users = FAKE_NPM_USERS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *users -= 1;
        if *users == 0 {
            let _ = fs::remove_dir_all(&self.folder);
        }
    }
}

fn kill(pid: u32) {
    #[cfg(windows)]
    let _ = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .output();
    #[cfg(unix)]
    let _ = std::process::Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status();
}

fn workspace_declaring(label: &str, libraries: &[&str]) -> (TempDir, Workspace, UpdateConfig) {
    let root = TempDir::new(label);
    let dependencies: Vec<(&str, &str)> = libraries.iter().map(|name| (*name, "*")).collect();
    root.write("app/package.json", &manifest("app", "1.0.0", &dependencies));
    let config = UpdateConfig {
        projects: vec![ConfiguredProject::new("app", "app")],
        packages: libraries
            .iter()
            .map(|name| PackageSpec::new(*name, "1.0.0"))
            .collect(),
        ..UpdateConfig::default()
    };
    let workspace = Workspace::open(root.path()).unwrap();
    (root, workspace, config)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_lookup_that_times_out_leaves_no_npm_process_behind() {
    let npm = fake_npm();
    let name = format!("hang-timeout-{}", std::process::id());
    let (_root, workspace, config) = workspace_declaring("latest-timeout", &[&name]);
    let drift = read_dependency_drift(&workspace, &config);
    let lookup = LatestLookup {
        timeout: Duration::from_secs(3),
        ..LatestLookup::new(DependencyRequest::new())
    };

    let latest = fetch_latest_versions(&workspace, &config, &drift, &lookup).await;

    let survivors: Vec<u32> = npm
        .started_pids(&name)
        .into_iter()
        .filter(|pid| is_alive(*pid))
        .collect();
    survivors.iter().for_each(|pid| kill(*pid));
    assert_eq!(latest[0].version, None);
    assert_eq!(
        latest[0].error.as_deref(),
        Some("npm view took longer than 3 seconds")
    );
    assert_eq!(npm.started_pids(&name).len(), 1);
    assert_eq!(survivors, Vec::<u32>::new(), "npm processes left running");
}

#[tokio::test(flavor = "multi_thread")]
async fn cancelling_the_request_stops_running_and_queued_lookups() {
    let npm = fake_npm();
    let names: Vec<String> = (0..6)
        .map(|index| format!("hang-cancel-{}-{index}", std::process::id()))
        .collect();
    let libraries: Vec<&str> = names.iter().map(String::as_str).collect();
    let (_root, workspace, config) = workspace_declaring("latest-cancel", &libraries);
    let drift = read_dependency_drift(&workspace, &config);
    let request = DependencyRequest::new();
    let lookup = LatestLookup {
        limit: 2,
        ..LatestLookup::new(request.clone())
    };
    let started = |names: &[String]| -> Vec<u32> {
        names
            .iter()
            .flat_map(|name| npm.started_pids(name))
            .collect()
    };

    let mut fetching = tokio::spawn({
        let (workspace, config) = (workspace.clone(), config.clone());
        async move { fetch_latest_versions(&workspace, &config, &drift, &lookup).await }
    });
    let waiting_since = Instant::now();
    while started(&names).len() < 2 && waiting_since.elapsed() < Duration::from_secs(20) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    request.cancel();
    let finished = tokio::time::timeout(Duration::from_secs(10), &mut fetching).await;
    if finished.is_err() {
        fetching.abort();
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    let pids = started(&names);
    let survivors: Vec<u32> = pids.iter().copied().filter(|pid| is_alive(*pid)).collect();
    survivors.iter().for_each(|pid| kill(*pid));
    let latest = finished
        .expect("cancelled lookups end within 10 seconds")
        .unwrap();
    assert_eq!(pids.len(), 2, "queued lookups started npm: {pids:?}");
    assert_eq!(survivors, Vec::<u32>::new(), "npm processes left running");
    assert_eq!(latest.len(), 6);
    assert!(latest.iter().all(|entry| entry.version.is_none()));
}

#[test]
fn a_cancelled_request_ends_the_duplicate_walk_without_results() {
    let root = TempDir::new("copies-cancelled");
    root.write(
        "app/package.json",
        &manifest("app", "1.0.0", &[("@acme/core", "*")]),
    );
    for path in [
        "app/node_modules/@acme/core",
        "app/node_modules/host/node_modules/@acme/core",
    ] {
        root.write(
            &format!("{path}/package.json"),
            &manifest("@acme/core", "1.0.0", &[]),
        );
    }
    let config = UpdateConfig {
        projects: vec![ConfiguredProject::new("app", "app")],
        packages: vec![PackageSpec::new("@acme/core", "1.0.0")],
        ..UpdateConfig::default()
    };
    let workspace = Workspace::open(root.path()).unwrap();
    let running = DependencyRequest::new();
    let cancelled = DependencyRequest::new();
    cancelled.cancel();

    let found = find_installed_copies(&workspace, &config, CopyScope::Libraries, &running);
    let after_cancel = find_installed_copies(&workspace, &config, CopyScope::Libraries, &cancelled);

    assert_eq!(found[0].packages.len(), 1);
    assert!(after_cancel
        .iter()
        .all(|project| project.packages.is_empty()));
}

#[test]
fn the_request_table_cancels_by_id_even_when_the_cancel_arrives_first() {
    let requests = DependencyRequests::default();
    let [first, second, third, fourth] = [(); 4].map(|_| requests.open());

    let running = requests.start(first);
    let other = requests.start(second);
    requests.cancel(first);
    requests.cancel(third);
    let cancelled_early = requests.start(third);
    requests.finish(second, &other);
    requests.cancel(second);
    let closing = requests.start(fourth);
    requests.cancel_all();
    let after_close = requests.open();

    assert!(running.is_cancelled());
    assert!(!other.is_cancelled());
    assert!(cancelled_early.is_cancelled());
    assert!(closing.is_cancelled());
    assert!(!requests.start(after_close).is_cancelled());
}

#[test]
fn a_cancel_still_lands_when_a_newer_request_started_first() {
    let requests = DependencyRequests::default();
    let older_id = requests.open();
    let newer_id = requests.open();

    let _newer = requests.start(newer_id);
    requests.cancel(older_id);
    let older = requests.start(older_id);

    assert!(older.is_cancelled());
}

#[test]
fn finishing_a_replayed_id_does_not_detach_the_running_request() {
    let requests = DependencyRequests::default();
    let id = requests.open();
    let running = requests.start(id);
    let replayed = requests.start(id);

    requests.finish(id, &replayed);
    requests.cancel(id);

    assert!(replayed.is_cancelled());
    assert!(running.is_cancelled());
}
