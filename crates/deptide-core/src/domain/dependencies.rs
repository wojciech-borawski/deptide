use serde::{Deserialize, Serialize};

use super::DependencySection;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyDrift {
    pub projects: Vec<String>,
    pub unreadable: Vec<String>,
    pub libraries: Vec<LibraryDrift>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryDrift {
    pub name: String,
    pub local_project: Option<String>,
    pub local_version: Option<String>,
    pub ambiguous_projects: Vec<String>,
    pub newest_installed: Option<String>,
    pub latest: Option<String>,
    pub latest_error: Option<String>,
    pub mismatch: bool,
    pub cells: Vec<DriftCell>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriftCell {
    pub project: String,
    pub section: DependencySection,
    pub declared: String,
    pub installed: Option<String>,
    pub out_of_range: bool,
    pub differs: bool,
    pub behind: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LatestVersion {
    pub name: String,
    pub version: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub cycles: Vec<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    pub name: String,
    pub package_name: Option<String>,
    pub layer: u32,
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
    pub consumer: String,
    pub library: String,
    pub section: DependencySection,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CopyScope {
    #[default]
    Libraries,
    AllPackages,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCopies {
    pub project: String,
    pub has_node_modules: bool,
    pub packages: Vec<PackageCopies>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageCopies {
    pub name: String,
    pub copies: Vec<InstalledCopy>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledCopy {
    pub version: Option<String>,
    pub path: String,
}
