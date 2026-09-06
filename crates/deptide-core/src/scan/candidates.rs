use std::collections::HashMap;

use super::git::{branch_suffix, compile_branch_pattern};
use crate::domain::{DependencyCandidate, DetectedProject};

fn clean_range(range: &str) -> String {
    range
        .trim_start_matches(|c: char| matches!(c, '^' | '~' | '>' | '=' | '<') || c.is_whitespace())
        .trim()
        .to_string()
}

struct LocalLibrary<'a> {
    version: &'a str,
    branch: Option<&'a str>,
}

pub fn collect_dependency_candidates(
    all_projects: &[DetectedProject],
    consumers: &[DetectedProject],
    branch_pattern: &str,
) -> Vec<DependencyCandidate> {
    let pattern = compile_branch_pattern(branch_pattern);

    let locals: HashMap<&str, LocalLibrary> = all_projects
        .iter()
        .filter_map(|project| {
            Some((
                project.package_name.as_deref()?,
                LocalLibrary {
                    version: project.version.as_deref()?,
                    branch: project.branch.as_deref(),
                },
            ))
        })
        .collect();

    let mut candidates: Vec<DependencyCandidate> = Vec::new();

    let mut register = |project: &DetectedProject, name: &str, range: &str, is_dev: bool| {
        let position = match candidates.iter().position(|entry| entry.name == name) {
            Some(position) => position,
            None => {
                let local = locals.get(name);
                candidates.push(DependencyCandidate {
                    name: name.to_string(),
                    local_version: local.map(|entry| entry.version.to_string()),
                    local_branch: local.and_then(|entry| entry.branch.map(str::to_string)),
                    branch_suffix: local
                        .and_then(|entry| entry.branch)
                        .and_then(|branch| branch_suffix(branch, &pattern)),
                    current_ranges: Vec::new(),
                    used_by: Vec::new(),
                    is_dev_dependency: is_dev,
                });
                candidates.len() - 1
            }
        };

        let candidate = &mut candidates[position];
        let cleaned = clean_range(range);
        if !cleaned.is_empty() && !candidate.current_ranges.contains(&cleaned) {
            candidate.current_ranges.push(cleaned);
        }
        if !candidate.used_by.contains(&project.proposed_name) {
            candidate.used_by.push(project.proposed_name.clone());
        }
        candidate.is_dev_dependency = candidate.is_dev_dependency && is_dev;
    };

    for project in consumers {
        for (name, range) in &project.dependencies {
            register(project, name, range, false);
        }
        for (name, range) in &project.dev_dependencies {
            register(project, name, range, true);
        }
    }

    candidates.sort_by(|a, b| {
        b.local_version
            .is_some()
            .cmp(&a.local_version.is_some())
            .then_with(|| a.name.cmp(&b.name))
    });

    candidates
}

pub fn strip_prerelease(version: &str) -> String {
    let trimmed = version.trim();
    let mut end = 0;
    let mut dots = 0;

    for (index, character) in trimmed.char_indices() {
        if character.is_ascii_digit() {
            end = index + 1;
        } else if character == '.' && dots < 2 {
            dots += 1;
            end = index + 1;
        } else {
            break;
        }
    }

    let core = trimmed[..end].trim_end_matches('.');
    let is_semver_core =
        core.split('.').count() == 3 && core.split('.').all(|part| !part.is_empty());

    if is_semver_core {
        core.to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn apply_suffix(base_version: &str, suffix: &str) -> String {
    let cleaned_suffix = suffix.trim().trim_start_matches('-');
    let base = strip_prerelease(base_version);

    if cleaned_suffix.is_empty() {
        base
    } else {
        format!("{base}-{cleaned_suffix}")
    }
}
