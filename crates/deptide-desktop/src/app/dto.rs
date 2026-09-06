use serde::{Deserialize, Serialize};

use deptide_core::domain::{
    DependencyCandidate, DetectedProject, ProjectKind, RunSnapshot, RunSummary, Settings,
    UpdateConfig,
};
use deptide_core::workspace::{RecentWorkspace, SavedRunFile};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectView {
    pub name: String,
    pub path: String,
    pub absolute_path: String,
    pub exists: bool,
    pub kind: ProjectKind,
    pub skip: bool,
    pub packages: Option<Vec<String>>,
    pub install_args: Option<Vec<String>>,
    pub duplicate_of: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSnapshot {
    pub root: String,
    pub config_file: String,
    pub config: UpdateConfig,
    pub projects: Vec<ProjectView>,
    pub settings: Settings,
    pub saved_runs: Vec<SavedRunFile>,
    pub history: Vec<RunSummary>,
    pub recent: Vec<RecentWorkspace>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScannedProject {
    #[serde(flatten)]
    pub detected: DetectedProject,
    pub config_path: String,
    pub configured_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub projects_root: String,
    pub depth: u32,
    pub projects: Vec<ScannedProject>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInspection {
    pub candidates: Vec<DependencyCandidate>,
    pub unreadable: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunStartOutcome {
    pub run_id: String,
    pub snapshot: RunSnapshot,
    pub missing: Vec<String>,
    pub without_packages: Vec<String>,
    pub saved_run_path: Option<String>,
}
