use std::path::PathBuf;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::sleep;

use super::projects::{installed_version, WorkspaceProjects};
use super::request::LatestLookup;
use crate::domain::{DependencyDrift, DriftCell, LatestVersion, LibraryDrift, UpdateConfig};
use crate::execution::{describe_exit_code, npm_program, run_command, CommandOutcome, CommandSpec};
use crate::util::semver::{Range, Version};
use crate::workspace::Workspace;

const VIEW_ERROR_LINES: usize = 2;

fn is_out_of_range(installed: Option<&str>, declared: &str) -> bool {
    let (Some(installed), Some(range)) = (installed, Range::parse(declared)) else {
        return false;
    };
    Version::parse(installed).is_some_and(|version| !range.satisfies(&version))
}

fn newest_installed(cells: &[DriftCell]) -> Option<String> {
    cells
        .iter()
        .filter_map(|cell| {
            let installed = cell.installed.as_deref()?;
            Version::parse(installed).map(|version| (version, installed))
        })
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, installed)| installed.to_string())
}

fn mark_mismatches(cells: &mut [DriftCell]) {
    let mut versions: Vec<&str> = cells
        .iter()
        .filter_map(|cell| cell.installed.as_deref())
        .collect();
    versions.sort_unstable();
    versions.dedup();
    let versions_differ = versions.len() > 1;

    for cell in cells.iter_mut() {
        cell.out_of_range = is_out_of_range(cell.installed.as_deref(), &cell.declared);
        cell.differs = versions_differ && cell.installed.is_some();
    }
}

pub fn read_dependency_drift(workspace: &Workspace, config: &UpdateConfig) -> DependencyDrift {
    let read = WorkspaceProjects::read(workspace, config);
    let providers = read.providers();

    let libraries = read
        .library_names(config)
        .into_iter()
        .map(|name| {
            let local = providers.single.get(name.as_str());
            let mut cells: Vec<DriftCell> = read
                .projects
                .iter()
                .filter_map(|project| {
                    let (section, declared) = project.declared(&name)?;
                    Some(DriftCell {
                        project: project.name().to_string(),
                        section,
                        declared: declared.to_string(),
                        installed: installed_version(&project.directory, &name),
                        out_of_range: false,
                        differs: false,
                        behind: false,
                    })
                })
                .collect();
            mark_mismatches(&mut cells);

            LibraryDrift {
                local_project: local.map(|project| project.name().to_string()),
                local_version: local.and_then(|project| project.detected.version.clone()),
                ambiguous_projects: providers
                    .ambiguous
                    .get(name.as_str())
                    .map(|found| {
                        found
                            .iter()
                            .map(|project| project.name().to_string())
                            .collect()
                    })
                    .unwrap_or_default(),
                name,
                newest_installed: newest_installed(&cells),
                latest: None,
                latest_error: None,
                mismatch: cells.iter().any(|cell| cell.out_of_range || cell.differs),
                cells,
            }
        })
        .collect();

    DependencyDrift {
        projects: read.names(),
        unreadable: read.unreadable,
        libraries,
    }
}

fn is_behind(installed: Option<&str>, latest: Option<&str>) -> bool {
    let (Some(installed), Some(latest)) = (installed, latest) else {
        return false;
    };
    match (Version::parse(installed), Version::parse(latest)) {
        (Some(installed), Some(latest)) => installed < latest,
        _ => false,
    }
}

pub fn apply_latest_versions(drift: &mut DependencyDrift, latest: &[LatestVersion]) {
    for library in &mut drift.libraries {
        let Some(found) = latest.iter().find(|entry| entry.name == library.name) else {
            continue;
        };
        library.latest = found.version.clone();
        library.latest_error = found.error.clone();
        for cell in &mut library.cells {
            cell.behind = is_behind(cell.installed.as_deref(), library.latest.as_deref());
        }
    }
}

fn parse_view_output(stdout: &[String]) -> Option<String> {
    match serde_json::from_str::<Value>(&stdout.join(
        "
",
    ))
    .ok()?
    {
        Value::String(version) => Some(version),
        Value::Array(versions) => versions.last()?.as_str().map(str::to_string),
        _ => None,
    }
}

fn describe_view_failure(outcome: &CommandOutcome, messages: &[String]) -> String {
    if let Some(error) = &outcome.spawn_error {
        return format!("npm could not be started: {error}");
    }
    let errors: Vec<&str> = messages
        .iter()
        .map(|line| line.trim())
        .filter(|line| line.starts_with("npm error") || line.starts_with("npm ERR!"))
        .take(VIEW_ERROR_LINES)
        .collect();
    if errors.is_empty() {
        format!(
            "npm view exited with code {}",
            describe_exit_code(outcome.code)
        )
    } else {
        errors.join("; ")
    }
}

struct ViewTarget {
    name: String,
    directory: PathBuf,
    job: usize,
}

async fn view_latest(target: ViewTarget, lookup: LatestLookup) -> LatestVersion {
    let ViewTarget {
        name,
        directory,
        job,
    } = target;
    let spec = CommandSpec {
        program: npm_program().to_string(),
        args: vec![
            "view".to_string(),
            name.clone(),
            "version".to_string(),
            "--json".to_string(),
        ],
        cwd: directory,
        shell: None,
        capture_stdout: true,
    };
    let processes = lookup.request.processes();
    let mut messages = Vec::new();
    let mut timed_out = false;
    let outcome = {
        let viewed = run_command(&spec, processes, job, |line| messages.push(line));
        tokio::pin!(viewed);
        tokio::select! {
            outcome = &mut viewed => outcome,
            () = sleep(lookup.timeout) => {
                timed_out = true;
                processes.abort_job(job);
                viewed.await
            }
        }
    };

    let (version, error) = if lookup.request.is_cancelled() {
        (None, Some("npm view was cancelled".to_string()))
    } else if timed_out {
        (
            None,
            Some(format!(
                "npm view took longer than {} seconds",
                lookup.timeout.as_secs()
            )),
        )
    } else {
        match parse_view_output(&outcome.stdout) {
            Some(version) if outcome.code == Some(0) => (Some(version), None),
            _ => (None, Some(describe_view_failure(&outcome, &messages))),
        }
    };
    LatestVersion {
        name,
        version,
        error,
    }
}

pub async fn fetch_latest_versions(
    workspace: &Workspace,
    config: &UpdateConfig,
    drift: &DependencyDrift,
    lookup: &LatestLookup,
) -> Vec<LatestVersion> {
    let permits = Arc::new(Semaphore::new(lookup.limit.max(1)));
    let mut lookups = JoinSet::new();

    for (index, library) in drift.libraries.iter().enumerate() {
        let directory = library
            .cells
            .first()
            .and_then(|cell| {
                config
                    .projects
                    .iter()
                    .find(|project| project.name == cell.project)
            })
            .map(|project| workspace.resolve_project(&project.path))
            .unwrap_or_else(|| workspace.root().to_path_buf());
        let target = ViewTarget {
            name: library.name.clone(),
            directory,
            job: index,
        };
        let permits = Arc::clone(&permits);
        let lookup = lookup.clone();
        lookups.spawn(async move {
            let _permit = permits.acquire_owned().await;
            (index, view_latest(target, lookup).await)
        });
    }

    let mut found: Vec<(usize, LatestVersion)> = Vec::new();
    while let Some(joined) = lookups.join_next().await {
        if let Ok(entry) = joined {
            found.push(entry);
        }
    }
    found.sort_by_key(|(index, _)| *index);
    found.into_iter().map(|(_, entry)| entry).collect()
}
