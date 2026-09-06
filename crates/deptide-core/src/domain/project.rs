use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfiguredProject {
    pub name: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub packages: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub skip: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_args: Option<Vec<String>>,
}

impl ConfiguredProject {
    pub fn new(name: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
            packages: None,
            skip: false,
            install_args: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageManifest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub scripts: BTreeMap<String, String>,
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
    #[serde(default)]
    pub dev_dependencies: BTreeMap<String, String>,
    #[serde(default)]
    pub peer_dependencies: BTreeMap<String, String>,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub main: Option<Value>,
    #[serde(default)]
    pub module: Option<Value>,
    #[serde(default)]
    pub exports: Option<Value>,
    #[serde(default)]
    pub types: Option<Value>,
    #[serde(default)]
    pub typings: Option<Value>,
    #[serde(default)]
    pub files: Option<Value>,
    #[serde(default)]
    pub publish_config: Option<Value>,
    #[serde(default)]
    pub bin: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ProjectKind {
    Library,
    Application,
    #[default]
    Unknown,
}

impl PackageManifest {
    pub fn kind(&self) -> ProjectKind {
        let library_signals = [
            self.main.is_some(),
            self.module.is_some(),
            self.exports.is_some(),
            self.types.is_some(),
            self.typings.is_some(),
            self.files.is_some(),
            self.publish_config.is_some(),
            self.bin.is_some(),
            !self.peer_dependencies.is_empty(),
        ];

        if library_signals.iter().any(|signal| *signal) {
            ProjectKind::Library
        } else {
            ProjectKind::Application
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedProject {
    pub directory: String,
    pub relative_path: String,
    pub proposed_name: String,
    pub package_name: Option<String>,
    pub version: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub kind: ProjectKind,
    pub dependencies: BTreeMap<String, String>,
    pub dev_dependencies: BTreeMap<String, String>,
    pub has_build_script: bool,
}
