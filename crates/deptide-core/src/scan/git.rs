use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use regex::Regex;

use crate::domain::{LookupCause, PackageManifest};
use crate::scan::MANIFEST_FILE_NAME;

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
    run_git_bytes(directory, args, input).map(|bytes| String::from_utf8_lossy(&bytes).to_string())
}

fn run_git_bytes(directory: &Path, args: &[&str], input: Option<&str>) -> Option<Vec<u8>> {
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

    output.status.success().then_some(output.stdout)
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

/// The project's version where the current branch left its base, or why it is unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForkPointVersion {
    Found {
        version: String,
        base_branch: String,
        commit: String,
    },
    Unavailable(LookupCause),
}

/// The `package.json` version where the current branch left the branch it was created from;
/// its own remote copies and local branches created after it never count as that base.
pub fn version_at_fork_point(directory: &Path) -> ForkPointVersion {
    find_version_at_fork_point(directory).unwrap_or_else(ForkPointVersion::Unavailable)
}

fn find_version_at_fork_point(directory: &Path) -> Result<ForkPointVersion, LookupCause> {
    let fork = find_fork(directory)?;
    let version = fork.version()?;

    Ok(ForkPointVersion::Found {
        version,
        base_branch: fork.base_branch,
        commit: fork.short_commit,
    })
}

/// What the current branch's own commits did to the project's `package.json` version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BranchVersionChange {
    /// No commit of the branch changed the version; `version` is the committed one at HEAD.
    Unchanged {
        version: String,
        base_branch: String,
        commit: String,
    },
    /// The oldest commit of the branch that changed the version, and the change it made.
    Changed {
        from: String,
        to: String,
        commit: String,
    },
    Unavailable(LookupCause),
}

/// Whether any commit the current branch made since it left its base changed the
/// `package.json` version. A merge that only brings in another branch's version,
/// such as a release bump merged from main, is not a change made by this branch.
/// Uncommitted edits are not looked at; compare the working tree with `version`.
pub fn version_change_on_branch(directory: &Path) -> BranchVersionChange {
    find_version_change_on_branch(directory).unwrap_or_else(BranchVersionChange::Unavailable)
}

fn find_version_change_on_branch(directory: &Path) -> Result<BranchVersionChange, LookupCause> {
    let fork = find_fork(directory)?;
    fork.version()?;

    // Only the line ending is removed: a folder name may start or end with a space.
    let prefix = run_git(directory, &["rev-parse", "--show-prefix"], None)
        .map(|text| text.trim_end_matches(['\r', '\n']).to_string())
        .unwrap_or_default();
    let mut commits: Vec<&str> = vec![&fork.head];
    for commit in &fork.own_commits {
        commits.push(&commit.id);
        commits.extend(commit.parents.iter().map(String::as_str));
    }
    let versions = committed_versions(
        directory,
        &format!("{prefix}{MANIFEST_FILE_NAME}"),
        &commits,
    )?;
    let version_at = |commit: &str| versions.get(commit).cloned().flatten();

    for commit in fork.own_commits.iter().rev() {
        let Some((first_parent, merged_parents)) = commit.parents.split_first() else {
            continue;
        };
        let before = version_at(first_parent);
        let after = version_at(&commit.id);
        if before == after {
            continue;
        }
        let merged_in = merged_parents
            .iter()
            .any(|parent| version_at(parent) == after);
        if merged_in {
            continue;
        }

        let none = || "none".to_string();
        return Ok(BranchVersionChange::Changed {
            from: before.unwrap_or_else(none),
            to: after.unwrap_or_else(none),
            commit: git_text(directory, &["rev-parse", "--short", &commit.id])
                .unwrap_or_else(|| commit.id.clone()),
        });
    }

    Ok(BranchVersionChange::Unchanged {
        version: version_at(&fork.head).ok_or(LookupCause::NoVersionInLastCommit)?,
        base_branch: fork.base_branch,
        commit: fork.short_commit,
    })
}

/// The `version` in `path` at each of `commits`, `None` where the file is missing or has none.
/// Two git calls whatever the number of commits: one names the file per commit, one reads
/// each distinct file content.
fn committed_versions(
    directory: &Path,
    path: &str,
    commits: &[&str],
) -> Result<HashMap<String, Option<String>>, LookupCause> {
    let unique: Vec<&str> = commits
        .iter()
        .copied()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let specs: String = unique
        .iter()
        .map(|commit| format!("{commit}:{path}\n"))
        .collect();
    let listing = run_git(
        directory,
        &["cat-file", "--batch-check=%(objectname) %(objecttype)"],
        Some(&specs),
    )
    .ok_or(LookupCause::HistoryUnreadable)?;
    let blob_of: HashMap<&str, String> = unique
        .iter()
        .zip(listing.lines())
        .filter_map(|(commit, line)| {
            let (object, kind) = line.split_once(' ')?;
            (kind == "blob").then(|| (*commit, object.to_string()))
        })
        .collect();

    let blobs: Vec<&String> = blob_of
        .values()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let requests: String = blobs.iter().map(|blob| format!("{blob}\n")).collect();
    let contents = if blobs.is_empty() {
        Vec::new()
    } else {
        run_git_bytes(directory, &["cat-file", "--batch"], Some(&requests))
            .ok_or(LookupCause::HistoryUnreadable)?
    };
    let version_of_blob = parse_batch_versions(&contents);

    Ok(unique
        .iter()
        .map(|commit| {
            let version = blob_of
                .get(commit)
                .and_then(|blob| version_of_blob.get(blob).cloned().flatten());
            (commit.to_string(), version)
        })
        .collect())
}

/// Reads `git cat-file --batch` output: per object a `<id> <type> <size>` line, the bytes and a newline.
fn parse_batch_versions(mut output: &[u8]) -> HashMap<String, Option<String>> {
    let mut versions = HashMap::new();
    while let Some(end) = output.iter().position(|byte| *byte == b'\n') {
        let header = String::from_utf8_lossy(&output[..end]).to_string();
        output = &output[end + 1..];
        let mut fields = header.split(' ');
        let (Some(object), Some(_), Some(size)) = (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let Ok(size) = size.parse::<usize>() else {
            continue;
        };
        let Some(content) = output.get(..size) else {
            break;
        };
        let version = serde_json::from_slice::<PackageManifest>(content)
            .ok()
            .and_then(|manifest| manifest.version);
        versions.insert(object.to_string(), version);
        output = output.get(size + 1..).unwrap_or_default();
    }
    versions
}

/// Where the current branch left its base, and the commits it made since.
struct Fork {
    directory: PathBuf,
    head: String,
    commit: String,
    short_commit: String,
    base_branch: String,
    /// The branch's own first-parent commits, newest first.
    own_commits: Vec<OwnCommit>,
}

struct OwnCommit {
    id: String,
    /// The first parent, then any merged parents.
    parents: Vec<String>,
}

impl Fork {
    fn version(&self) -> Result<String, LookupCause> {
        git_show(
            &self.directory,
            &format!("{}:./{MANIFEST_FILE_NAME}", self.commit),
        )
        .and_then(|text| serde_json::from_str::<PackageManifest>(&text).ok())
        .and_then(|manifest| manifest.version)
        .ok_or(LookupCause::NoVersionAtBase)
    }
}

fn find_fork(directory: &Path) -> Result<Fork, LookupCause> {
    if git_text(directory, &["rev-parse", "--is-inside-work-tree"]).as_deref() != Some("true") {
        return Err(LookupCause::NotARepository);
    }

    let head = git_text(directory, &["rev-parse", "--verify", "--quiet", "HEAD"])
        .filter(|commit| !commit.is_empty())
        .ok_or(LookupCause::NoCommits)?;
    let branch = git_text(directory, &["symbolic-ref", "--quiet", "HEAD"])
        .and_then(|reference| {
            reference
                .strip_prefix(LOCAL_BRANCH_PREFIX)
                .map(str::to_string)
        })
        .ok_or(LookupCause::DetachedHead)?;

    if MAIN_BRANCH_NAMES.contains(&branch.as_str()) {
        return Err(LookupCause::OnMainBranch);
    }

    let branch_created = created_at(directory, &format!("{LOCAL_BRANCH_PREFIX}{branch}"))
        .ok_or(LookupCause::NoReflog)?;
    let candidates = base_candidates(directory, &head, &branch, branch_created)?;
    if candidates.is_empty() {
        return Err(LookupCause::NoOtherBranch);
    }

    let own_commits = own_commits(directory, &head, &candidates)?;
    let commit = fork_commit(directory, &head, &own_commits)?;

    Ok(Fork {
        directory: directory.to_path_buf(),
        short_commit: git_text(directory, &["rev-parse", "--short", &commit])
            .unwrap_or_else(|| commit.clone()),
        base_branch: base_branch_name(directory, &commit, &candidates),
        head,
        commit,
        own_commits,
    })
}

/// Full names of the local and remote branches the current branch may have been created from.
fn base_candidates(
    directory: &Path,
    head: &str,
    branch: &str,
    branch_created: u64,
) -> Result<Vec<String>, LookupCause> {
    let tips = git_lines(
        directory,
        &[
            "for-each-ref",
            "--format=%(objectname) %(refname)",
            "refs/heads",
            "refs/remotes",
        ],
    )
    .ok_or(LookupCause::BranchListFailed)?;
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
    .ok_or(LookupCause::BranchListFailed)?
    .into_iter()
    .collect();
    let remotes = git_lines(directory, &["remote"]).ok_or(LookupCause::RemoteListFailed)?;
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

/// The first-parent commits of `head` that no candidate contains, newest first.
fn own_commits(
    directory: &Path,
    head: &str,
    candidates: &[String],
) -> Result<Vec<OwnCommit>, LookupCause> {
    let exclusions: String = candidates
        .iter()
        .map(|reference| format!("^{reference}\n"))
        .collect();
    let listing = run_git(
        directory,
        &["rev-list", "--first-parent", "--parents", head, "--stdin"],
        Some(&exclusions),
    )
    .ok_or(LookupCause::CommitListFailed)?;

    Ok(listing
        .lines()
        .filter_map(|line| {
            let mut ids = line.split_whitespace().map(str::to_string);
            Some(OwnCommit {
                id: ids.next()?,
                parents: ids.collect(),
            })
        })
        .collect())
}

/// The parent of the oldest own commit, or HEAD when the branch has none.
fn fork_commit(
    directory: &Path,
    head: &str,
    own_commits: &[OwnCommit],
) -> Result<String, LookupCause> {
    let Some(oldest) = own_commits.last() else {
        return Ok(head.to_string());
    };

    oldest.parents.first().cloned().ok_or_else(|| {
        if is_shallow(directory) {
            LookupCause::ShallowClone
        } else {
            LookupCause::NoSharedHistory
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
