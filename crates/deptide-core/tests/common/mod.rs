#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Once;

static COUNTER: AtomicUsize = AtomicUsize::new(0);
static GIT_CEILING: Once = Once::new();

/// A folder in the system temp folder. The first one sets `GIT_CEILING_DIRECTORIES` to the temp
/// folder for the whole test process, so no git call finds a repository above it.
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub fn new(label: &str) -> Self {
        GIT_CEILING
            .call_once(|| std::env::set_var("GIT_CEILING_DIRECTORIES", std::env::temp_dir()));
        let sequence = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "deptide-test-{label}-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temp dir can be created");
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn join(&self, relative: &str) -> PathBuf {
        self.path.join(relative)
    }

    pub fn write(&self, relative: &str, content: &str) -> PathBuf {
        let target = self.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).expect("parent can be created");
        }
        fs::write(&target, content).expect("file can be written");
        target
    }

    pub fn mkdir(&self, relative: &str) -> PathBuf {
        let target = self.join(relative);
        fs::create_dir_all(&target).expect("dir can be created");
        target
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Seconds since the epoch for 2026-01-01, the start of the `GitRepo` clock.
const GIT_CLOCK_START: u64 = 1_767_225_600;

/// A git repository in a `TempDir`. Its own `git` calls ignore the user's global and system
/// config and run on a clock that moves one second per call, so reflog entries follow call order;
/// the code under test still reads the user's config.
pub struct GitRepo {
    root: TempDir,
    directory: PathBuf,
    global_config: PathBuf,
    clock: AtomicUsize,
}

impl GitRepo {
    pub fn new(label: &str) -> Self {
        let root = TempDir::new(label);
        let global_config = root.write("gitconfig", "");
        let directory = root.mkdir("repo");
        let repo = Self {
            root,
            directory,
            global_config,
            clock: AtomicUsize::new(0),
        };
        repo.git(&["init", "-q", "-b", "main"]);
        repo.git(&["config", "user.name", "Deptide Test"]);
        repo.git(&["config", "user.email", "test@deptide.invalid"]);
        repo.git(&["config", "commit.gpgsign", "false"]);
        repo
    }

    pub fn path(&self) -> &Path {
        &self.directory
    }

    /// The temp folder around the repository, for a bare remote next to it.
    pub fn outside(&self) -> &Path {
        self.root.path()
    }

    /// Runs git in the repository and returns its trimmed stdout; panics on failure.
    pub fn git(&self, args: &[&str]) -> String {
        let tick = self.clock.fetch_add(1, Ordering::SeqCst) as u64;
        let date = format!("{} +0000", GIT_CLOCK_START + tick);
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(&self.directory)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", &self.global_config)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_DATE", &date)
            .env("GIT_COMMITTER_DATE", &date)
            .stdin(std::process::Stdio::null())
            .output()
            .expect("git can be started");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    /// Makes the next git call happen in the same second as the previous one.
    pub fn repeat_last_second(&self) {
        self.clock.fetch_sub(1, Ordering::SeqCst);
    }

    pub fn write(&self, relative: &str, content: &str) -> PathBuf {
        let target = self.directory.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).expect("parent can be created");
        }
        fs::write(&target, content).expect("file can be written");
        target
    }

    /// Writes `package.json` in `folder` ("" for the repository root) with
    /// `version`, without committing it.
    pub fn write_version(&self, folder: &str, version: &str) -> PathBuf {
        let relative = if folder.is_empty() {
            "package.json".to_string()
        } else {
            format!("{folder}/package.json")
        };
        self.write(
            &relative,
            &format!("{{\"name\": \"web\", \"version\": \"{version}\"}}\n"),
        )
    }

    pub fn commit_all(&self, message: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
    }

    pub fn commit_version(&self, folder: &str, version: &str) {
        self.write_version(folder, version);
        self.commit_all(&format!("version {version}"));
    }

    /// Commits a change that does not touch any `package.json`.
    pub fn commit_other(&self, name: &str) {
        self.write(&format!("{name}.txt"), name);
        self.commit_all(&format!("change {name}"));
    }

    pub fn short(&self, revision: &str) -> String {
        self.git(&["rev-parse", "--short", revision])
    }
}

pub fn manifest(name: &str, version: &str, dependencies: &[(&str, &str)]) -> String {
    let deps: Vec<String> = dependencies
        .iter()
        .map(|(dep, range)| format!("\"{dep}\": \"{range}\""))
        .collect();

    format!(
        "{{\"name\": \"{name}\", \"version\": \"{version}\", \"scripts\": {{\"build\": \"echo build\"}}, \"dependencies\": {{{}}}}}",
        deps.join(", ")
    )
}
