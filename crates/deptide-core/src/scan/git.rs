use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

pub const DEFAULT_BRANCH_SUFFIX_PATTERN: &str = r"^([A-Za-z]+-\d+)";

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
