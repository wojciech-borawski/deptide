use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;
use serde_json::{Map, Value};

use super::Workspace;
use crate::domain::{ConfiguredProject, PackageSpec, ProjectGroup, UpdateConfig};
use crate::error::AppResult;
use crate::util::json::{read_json, write_json};

#[derive(Deserialize)]
#[serde(untagged)]
enum RawPackage {
    Spec(String),
    Object {
        name: Option<String>,
        version: Option<String>,
        #[serde(default, rename = "saveDev")]
        save_dev: bool,
        #[serde(default, rename = "savePeer")]
        save_peer: bool,
        #[serde(flatten)]
        unknown: Map<String, Value>,
    },
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawProject {
    Path(String),
    Object {
        name: Option<String>,
        path: Option<String>,
        #[serde(default)]
        packages: Option<Vec<String>>,
        #[serde(default)]
        skip: bool,
        #[serde(default, rename = "installArgs")]
        install_args: Option<Vec<String>>,
        #[serde(flatten)]
        unknown: Map<String, Value>,
    },
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RawConfig {
    #[serde(default)]
    packages: Vec<RawPackage>,
    #[serde(default)]
    projects: Vec<RawProject>,
    #[serde(default)]
    groups: Vec<ProjectGroup>,
    #[serde(default)]
    install_args: Vec<String>,
    #[serde(default)]
    audit_fix_args: Vec<String>,
    #[serde(default)]
    transfer_ignore: Vec<String>,
    #[serde(flatten)]
    unknown: Map<String, Value>,
}

fn normalize_package(raw: RawPackage) -> Option<PackageSpec> {
    match raw {
        RawPackage::Spec(spec) => PackageSpec::parse(&spec),
        RawPackage::Object {
            name,
            version,
            save_dev,
            save_peer,
            ..
        } => Some(PackageSpec {
            name: name.filter(|value| !value.is_empty())?,
            version: version.filter(|value| !value.is_empty())?,
            save_dev,
            save_peer,
            pack: None,
        }),
    }
}

fn base_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

fn normalize_project(raw: RawProject) -> Option<ConfiguredProject> {
    match raw {
        RawProject::Path(path) => Some(ConfiguredProject::new(base_name(&path), path)),
        RawProject::Object {
            name,
            path,
            packages,
            skip,
            install_args,
            ..
        } => {
            let path = path.filter(|value| !value.is_empty())?;
            let mut project =
                ConfiguredProject::new(name.unwrap_or_else(|| base_name(&path)), path);
            project.packages = packages.filter(|list| !list.is_empty());
            project.skip = skip;
            project.install_args = install_args.filter(|list| !list.is_empty());
            Some(project)
        }
    }
}

fn parse_config(raw: RawConfig) -> UpdateConfig {
    UpdateConfig {
        packages: raw
            .packages
            .into_iter()
            .filter_map(normalize_package)
            .collect(),
        projects: raw
            .projects
            .into_iter()
            .filter_map(normalize_project)
            .collect(),
        groups: raw.groups,
        install_args: raw.install_args,
        audit_fix_args: raw.audit_fix_args,
        transfer_ignore: raw.transfer_ignore,
    }
}

pub fn parse_config_text(text: &str) -> AppResult<UpdateConfig> {
    let raw: RawConfig = serde_json::from_str(text)?;
    Ok(parse_config(raw))
}

pub fn load_config(workspace: &Workspace) -> AppResult<UpdateConfig> {
    let raw = read_json::<RawConfig>(&workspace.config_file())?.unwrap_or_default();
    Ok(parse_config(raw))
}

fn groups_of_configured_projects(config: &UpdateConfig) -> Vec<ProjectGroup> {
    config
        .groups
        .iter()
        .map(|group| ProjectGroup {
            name: group.name.clone(),
            projects: group
                .projects
                .iter()
                .filter(|name| config.projects.iter().any(|project| &project.name == *name))
                .cloned()
                .collect(),
        })
        .filter(|group| !group.projects.is_empty())
        .collect()
}

struct ProjectFields {
    name: Option<String>,
    path: String,
    unknown: Map<String, Value>,
}

struct UnknownFields {
    top_level: Map<String, Value>,
    packages: HashMap<String, Map<String, Value>>,
    projects: Vec<ProjectFields>,
}

fn unknown_fields(raw: RawConfig) -> UnknownFields {
    let packages = raw
        .packages
        .into_iter()
        .filter_map(|package| match package {
            RawPackage::Object {
                name: Some(name),
                unknown,
                ..
            } => Some((name, unknown)),
            _ => None,
        })
        .collect();
    let projects = raw
        .projects
        .into_iter()
        .filter_map(|project| match project {
            RawProject::Object {
                name,
                path: Some(path),
                unknown,
                ..
            } => Some(ProjectFields {
                name,
                path,
                unknown,
            }),
            _ => None,
        })
        .collect();

    UnknownFields {
        top_level: raw.unknown,
        packages,
        projects,
    }
}

fn unknown_fields_on_disk(workspace: &Workspace) -> Option<UnknownFields> {
    read_json::<RawConfig>(&workspace.config_file())
        .ok()
        .flatten()
        .map(unknown_fields)
}

fn add_missing(target: &mut Map<String, Value>, fields: &Map<String, Value>) {
    for (key, value) in fields {
        target.entry(key.clone()).or_insert_with(|| value.clone());
    }
}

fn add_missing_to_packages(
    entries: Option<&mut Value>,
    by_name: &HashMap<String, Map<String, Value>>,
) {
    let Some(Value::Array(entries)) = entries else {
        return;
    };
    for entry in entries {
        let Value::Object(entry) = entry else {
            continue;
        };
        let fields = entry
            .get("name")
            .and_then(Value::as_str)
            .and_then(|name| by_name.get(name));
        if let Some(fields) = fields {
            add_missing(entry, fields);
        }
    }
}

fn fields_of_project<'a>(
    on_disk: &'a [ProjectFields],
    name: &str,
    path: &str,
) -> Option<&'a Map<String, Value>> {
    let candidates: Vec<&ProjectFields> = on_disk
        .iter()
        .filter(|project| match &project.name {
            Some(known) => known == name,
            None => project.path == path,
        })
        .collect();
    let chosen = match candidates.as_slice() {
        [only] => Some(*only),
        _ => {
            let mut same_path = candidates.iter().filter(|project| project.path == path);
            match (same_path.next(), same_path.next()) {
                (Some(only), None) => Some(*only),
                _ => None,
            }
        }
    };
    chosen.map(|project| &project.unknown)
}

fn add_missing_to_projects(entries: Option<&mut Value>, on_disk: &[ProjectFields]) {
    let Some(Value::Array(entries)) = entries else {
        return;
    };
    for entry in entries {
        let Value::Object(entry) = entry else {
            continue;
        };
        let text = |key: &str| entry.get(key).and_then(Value::as_str).unwrap_or_default();
        if let Some(fields) = fields_of_project(on_disk, text("name"), text("path")) {
            add_missing(entry, fields);
        }
    }
}

pub fn save_config(workspace: &Workspace, config: &UpdateConfig) -> AppResult<()> {
    let config = UpdateConfig {
        groups: groups_of_configured_projects(config),
        ..config.clone()
    };
    let Value::Object(mut written) = serde_json::to_value(&config)? else {
        unreachable!("a config serializes to a JSON object");
    };
    if let Some(unknown) = unknown_fields_on_disk(workspace) {
        add_missing_to_packages(written.get_mut("packages"), &unknown.packages);
        add_missing_to_projects(written.get_mut("projects"), &unknown.projects);
        add_missing(&mut written, &unknown.top_level);
    }
    write_json(&workspace.config_file(), &written)
}
