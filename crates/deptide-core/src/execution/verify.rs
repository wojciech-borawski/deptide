use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use crate::domain::{DependencyChange, InstalledPackage, PackageManifest, PackageSpec};
use crate::scan::MANIFEST_FILE_NAME;
use crate::util::json::read_json;

const HIDDEN_LOCKFILE: &str = ".package-lock.json";

#[derive(Deserialize, Default)]
struct HiddenLockfile {
    #[serde(default)]
    packages: BTreeMap<String, LockEntry>,
}

#[derive(Deserialize, Default, Clone)]
struct LockEntry {
    #[serde(default)]
    integrity: Option<String>,
    #[serde(default)]
    resolved: Option<String>,
}

pub fn verify_installed(directory: &Path, packages: &[PackageSpec]) -> Vec<InstalledPackage> {
    let node_modules = directory.join("node_modules");
    let lockfile = read_json::<HiddenLockfile>(&node_modules.join(HIDDEN_LOCKFILE))
        .ok()
        .flatten()
        .unwrap_or_default();

    packages
        .iter()
        .map(|spec| {
            let installed = read_json::<PackageManifest>(
                &node_modules.join(&spec.name).join(MANIFEST_FILE_NAME),
            )
            .ok()
            .flatten()
            .and_then(|manifest| manifest.version);

            let entry = lockfile
                .packages
                .get(&format!("node_modules/{}", spec.name))
                .cloned()
                .unwrap_or_default();

            InstalledPackage {
                name: spec.name.clone(),
                expected: spec.version.clone(),
                matches: satisfies(installed.as_deref(), &spec.version),
                installed,
                integrity: entry.integrity,
                resolved: entry.resolved,
            }
        })
        .collect()
}

pub fn read_top_level_versions(directory: &Path) -> BTreeMap<String, String> {
    let lockfile =
        read_json::<HiddenLockfileVersions>(&directory.join("node_modules").join(HIDDEN_LOCKFILE))
            .ok()
            .flatten()
            .unwrap_or_default();

    lockfile
        .packages
        .into_iter()
        .filter_map(|(key, entry)| {
            let name = key.strip_prefix("node_modules/")?;
            if name.contains("/node_modules/") {
                return None;
            }
            Some((name.to_string(), entry.version?))
        })
        .collect()
}

pub fn diff_dependencies(
    before: &BTreeMap<String, String>,
    after: &BTreeMap<String, String>,
) -> Vec<DependencyChange> {
    let names: std::collections::BTreeSet<&String> = before.keys().chain(after.keys()).collect();

    names
        .into_iter()
        .filter(|name| before.get(*name) != after.get(*name))
        .map(|name| DependencyChange {
            name: name.clone(),
            before: before.get(name).cloned(),
            after: after.get(name).cloned(),
        })
        .collect()
}

#[derive(Deserialize, Default)]
struct HiddenLockfileVersions {
    #[serde(default)]
    packages: BTreeMap<String, VersionEntry>,
}

#[derive(Deserialize, Default)]
struct VersionEntry {
    #[serde(default)]
    version: Option<String>,
}

fn is_exact_version(version: &str) -> bool {
    version.chars().next().is_some_and(|c| c.is_ascii_digit())
        && !version.contains(['^', '~', '>', '<', '=', '*', ' ', '|'])
}

fn satisfies(installed: Option<&str>, expected: &str) -> bool {
    match installed {
        None => false,
        Some(version) if is_exact_version(expected) => version == expected,
        Some(_) => true,
    }
}

pub fn describe_installed(entry: &InstalledPackage) -> String {
    let version = entry.installed.as_deref().unwrap_or("nothing");
    let hash = entry
        .integrity
        .as_deref()
        .map(short_integrity)
        .map(|hash| format!(" ({hash})"))
        .unwrap_or_default();

    if entry.matches {
        format!("verified {}@{version}{hash}", entry.name)
    } else {
        format!(
            "installed {}@{version}{hash} but expected {}",
            entry.name, entry.expected
        )
    }
}

pub fn short_integrity(integrity: &str) -> String {
    let (algorithm, digest) = integrity.split_once('-').unwrap_or(("", integrity));
    let head: String = digest.chars().take(12).collect();

    if algorithm.is_empty() {
        head
    } else {
        format!("{algorithm}-{head}…")
    }
}
