use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use regex::Regex;

use crate::domain::PackageManifest;

pub const DEFAULT_BRANCH_SUFFIX_PATTERN: &str = r"^([A-Za-z]+-\d+)";

/// Branches tried, in order, when looking for "the main branch" of a project.
const MAIN_BRANCH_CANDIDATES: &[&str] = &["main", "master", "origin/main", "origin/master"];

/// Branches that are nobody's feature branch, so they have no base to compare with.
const MAIN_BRANCH_NAMES: &[&str] = &["main", "master"];

const LOCAL_BRANCH_PREFIX: &str = "refs/heads/";
const REMOTE_BRANCH_PREFIX: &str = "refs/remotes/";

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Runs git in `directory` with `input` on stdin; its stdout when git succeeds.
fn run_git(directory: &Path, args: &[&str], input: Option<&str>) -> Option<String> {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(directory)
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = command.spawn().ok()?;
    let stdin = child.stdin.take();
    let output = std::thread::scope(|scope| {
        if let (Some(mut stdin), Some(text)) = (stdin, input) {
            scope.spawn(move || stdin.write_all(text.as_bytes()));
        }
        child.wait_with_output()
    })
    .ok()?;

    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).to_string())
}

fn git_text(directory: &Path, args: &[&str]) -> Option<String> {
    run_git(directory, args, None).map(|text| text.trim().to_string())
}

fn git_lines(directory: &Path, args: &[&str]) -> Option<Vec<String>> {
    run_git(directory, args, None).map(|text| text.lines().map(str::to_string).collect())
}

fn git_show(directory: &Path, spec: &str) -> Option<String> {
    run_git(directory, &["show", spec], None)
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

/// The project's version where the current branch left its base, or why it is unknown,
/// worded to follow "cannot tell whether this branch bumped the version: ".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForkPointVersion {
    Found {
        version: String,
        base_branch: String,
        commit: String,
    },
    Unavailable(String),
}

/// The `package.json` version where the current branch left the branch it was created from;
/// its own remote copies and local branches created after it never count as that base.
pub fn version_at_fork_point(directory: &Path) -> ForkPointVersion {
    find_version_at_fork_point(directory).unwrap_or_else(ForkPointVersion::Unavailable)
}

fn find_version_at_fork_point(directory: &Path) -> Result<ForkPointVersion, String> {
    if git_text(directory, &["rev-parse", "--is-inside-work-tree"]).as_deref() != Some("true") {
        return Err("the folder is not a git repository or git is not installed".to_string());
    }

    let head = git_text(directory, &["rev-parse", "--verify", "--quiet", "HEAD"])
        .filter(|commit| !commit.is_empty())
        .ok_or("the repository has no commits yet")?;
    let branch = git_text(directory, &["symbolic-ref", "--quiet", "HEAD"])
        .and_then(|reference| {
            reference
                .strip_prefix(LOCAL_BRANCH_PREFIX)
                .map(str::to_string)
        })
        .ok_or("HEAD is detached, check out a branch")?;

    if MAIN_BRANCH_NAMES.contains(&branch.as_str()) {
        return Err("on the main branch, no base branch to compare with".to_string());
    }

    let branch_created = created_at(directory, &format!("{LOCAL_BRANCH_PREFIX}{branch}"))
        .ok_or("cannot tell when this branch was created, it has no reflog")?;
    let candidates = base_candidates(directory, &head, &branch, branch_created)?;
    if candidates.is_empty() {
        return Err("no other branch to compare with".to_string());
    }

    let fork = fork_commit(directory, &head, &candidates)?;
    let version = git_show(directory, &format!("{fork}:./package.json"))
        .and_then(|text| serde_json::from_str::<PackageManifest>(&text).ok())
        .and_then(|manifest| manifest.version)
        .ok_or("package.json has no version at the base commit")?;

    Ok(ForkPointVersion::Found {
        version,
        base_branch: base_branch_name(directory, &fork, &candidates),
        commit: git_text(directory, &["rev-parse", "--short", &fork]).unwrap_or(fork),
    })
}

/// Full names of the local and remote branches the current branch may have been created from.
fn base_candidates(
    directory: &Path,
    head: &str,
    branch: &str,
    branch_created: u64,
) -> Result<Vec<String>, String> {
    let branch_list_failed = || "could not list the branches".to_string();
    let tips = git_lines(
        directory,
        &[
            "for-each-ref",
            "--format=%(objectname) %(refname)",
            "refs/heads",
            "refs/remotes",
        ],
    )
    .ok_or_else(branch_list_failed)?;
    let containing_head: HashSet<String> = git_lines(
        directory,
        &[
            "for-each-ref",
            "--contains",
            head,
            "--format=%(refname)",
            "refs/heads",
            "refs/remotes",
        ],
    )
    .ok_or_else(branch_list_failed)?
    .into_iter()
    .collect();
    let remotes = git_lines(directory, &["remote"])
        .ok_or_else(|| "could not list the remotes".to_string())?;
    let own_references = own_branch_references(branch, &remotes);
    let remote_heads: HashSet<String> = remotes
        .iter()
        .map(|remote| format!("{REMOTE_BRANCH_PREFIX}{remote}/HEAD"))
        .collect();

    Ok(tips
        .iter()
        .filter_map(|line| line.split_once(' '))
        .filter(|(tip, reference)| {
            if own_references.contains(*reference) || remote_heads.contains(*reference) {
                return false;
            }
            if tip != &head && containing_head.contains(*reference) {
                return false;
            }
            if !reference.starts_with(LOCAL_BRANCH_PREFIX) {
                return true;
            }
            match created_at(directory, reference) {
                Some(created) => created <= branch_created,
                None => true,
            }
        })
        .map(|(_, reference)| reference.to_string())
        .collect())
}

/// The current branch and its copies on every remote.
fn own_branch_references(branch: &str, remotes: &[String]) -> HashSet<String> {
    std::iter::once(format!("{LOCAL_BRANCH_PREFIX}{branch}"))
        .chain(
            remotes
                .iter()
                .map(|remote| format!("{REMOTE_BRANCH_PREFIX}{remote}/{branch}")),
        )
        .collect()
}

/// Seconds since the epoch of the oldest reflog entry of `reference`.
fn created_at(directory: &Path, reference: &str) -> Option<u64> {
    let entries = git_lines(
        directory,
        &[
            "reflog",
            "show",
            "--date=unix",
            "--format=%gd",
            reference,
            "--",
        ],
    )?;
    let (_, stamp) = entries.last()?.rsplit_once("@{")?;
    stamp.strip_suffix('}')?.parse().ok()
}

/// The parent of the oldest first-parent commit no candidate contains, or HEAD when there is none.
fn fork_commit(directory: &Path, head: &str, candidates: &[String]) -> Result<String, String> {
    let exclusions: String = candidates
        .iter()
        .map(|reference| format!("^{reference}\n"))
        .collect();
    let own_commits = run_git(
        directory,
        &["rev-list", "--first-parent", head, "--stdin"],
        Some(&exclusions),
    )
    .ok_or("could not list the commits of this branch")?;

    let Some(oldest) = own_commits.lines().last() else {
        return Ok(head.to_string());
    };

    git_text(
        directory,
        &["rev-parse", "--verify", "--quiet", &format!("{oldest}^1")],
    )
    .filter(|parent| !parent.is_empty())
    .ok_or_else(|| {
        if is_shallow(directory) {
            "the clone is shallow, fetch the full history".to_string()
        } else {
            "this branch shares no history with any other branch".to_string()
        }
    })
}

fn is_shallow(directory: &Path) -> bool {
    git_text(directory, &["rev-parse", "--is-shallow-repository"]).as_deref() == Some("true")
}

/// The shortest name among the candidates that contain `fork`, for the log.
fn base_branch_name(directory: &Path, fork: &str, candidates: &[String]) -> String {
    let candidates: HashSet<&str> = candidates.iter().map(String::as_str).collect();

    git_lines(
        directory,
        &[
            "for-each-ref",
            "--contains",
            fork,
            "--format=%(refname)",
            "refs/heads",
            "refs/remotes",
        ],
    )
    .unwrap_or_default()
    .iter()
    .filter(|reference| candidates.contains(reference.as_str()))
    .map(|reference| {
        reference
            .strip_prefix(LOCAL_BRANCH_PREFIX)
            .or_else(|| reference.strip_prefix(REMOTE_BRANCH_PREFIX))
            .unwrap_or(reference)
            .to_string()
    })
    .min_by_key(|name| (name.len(), name.clone()))
    .unwrap_or_else(|| "another branch".to_string())
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
