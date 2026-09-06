use std::path::Path;

use serde::Deserialize;

use super::Workspace;
use crate::domain::{ConfiguredProject, PackageSpec, UpdateConfig};
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
    install_args: Vec<String>,
    #[serde(default)]
    audit_fix_args: Vec<String>,
    #[serde(default)]
    transfer_ignore: Vec<String>,
}

fn normalize_package(raw: RawPackage) -> Option<PackageSpec> {
    match raw {
        RawPackage::Spec(spec) => PackageSpec::parse(&spec),
        RawPackage::Object {
            name,
            version,
            save_dev,
        } => Some(PackageSpec {
            name: name.filter(|value| !value.is_empty())?,
            version: version.filter(|value| !value.is_empty())?,
            save_dev,
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

pub fn save_config(workspace: &Workspace, config: &UpdateConfig) -> AppResult<()> {
    write_json(&workspace.config_file(), config)
}
