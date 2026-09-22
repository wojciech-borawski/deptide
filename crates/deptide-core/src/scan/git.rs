use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use regex::Regex;

use crate::domain::PackageManifest;

pub const DEFAULT_BRANCH_SUFFIX_PATTERN: &str = r"^([A-Za-z]+-\d+)";

/// Branches tried, in order, when looking for "the main branch" of a project.
const MAIN_BRANCH_CANDIDATES: &[&str] = &["main", "master", "origin/main", "origin/master"];

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn git_show(directory: &Path, spec: &str) -> Option<String> {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(directory)
        .arg("show")
        .arg(spec)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let output = command.output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).to_string())
}

/// The `version` of the project's `package.json` as committed on the main
/// branch, or `None` when git is unavailable, the folder is not a repository,
/// no main-like branch exists or the manifest is not tracked there.
pub fn version_on_main_branch(directory: &Path) -> Option<String> {
    MAIN_BRANCH_CANDIDATES.iter().find_map(|branch| {
        // `./package.json` is resolved by git relative to the working directory,
        // so this also works for projects nested inside a larger repository.
        let text = git_show(directory, &format!("{branch}:./package.json"))?;
        serde_json::from_str::<PackageManifest>(&text)
            .ok()
            .and_then(|manifest| manifest.version)
    })
}

fn git_directory(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start);

    while let Some(directory) = current {
        let candidate = directory.join(".git");

        if candidate.is_dir() {
            return Some(candidate);
        }

        if candidate.is_file() {
            let content = fs::read_to_string(&candidate).ok()?;
            let target = content.trim().strip_prefix("gitdir:")?.trim();
            let resolved = directory.join(target);
            return Some(resolved);
        }

        current = directory.parent();
    }

    None
}

pub fn current_branch(directory: &Path) -> Option<String> {
    let head = fs::read_to_string(git_directory(directory)?.join("HEAD")).ok()?;
    let reference = head.trim().strip_prefix("ref:")?.trim();

    reference
        .strip_prefix("refs/heads/")
        .map(|branch| branch.to_string())
}

pub fn compile_branch_pattern(pattern: &str) -> Regex {
    let trimmed = pattern.trim();
    let chosen = if trimmed.is_empty() {
        DEFAULT_BRANCH_SUFFIX_PATTERN
    } else {
        trimmed
    };

    Regex::new(chosen)
        .or_else(|_| Regex::new(DEFAULT_BRANCH_SUFFIX_PATTERN))
        .expect("the default branch pattern is valid")
}

pub fn branch_suffix(branch: &str, pattern: &Regex) -> Option<String> {
    let captures = pattern.captures(branch)?;
    let matched = captures
        .get(1)
        .or_else(|| captures.get(0))
        .map(|group| group.as_str().trim())?;

    if matched.is_empty() {
        None
    } else {
        Some(matched.to_string())
    }
}
