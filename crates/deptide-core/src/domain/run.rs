use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::PackageSpec;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StepName {
    Uninstall,
    Install,
    #[serde(rename = "force-install")]
    ForceInstall,
    Version,
    Audit,
    Build,
}

impl StepName {
    pub const fn all() -> [StepName; 6] {
        [
            StepName::Uninstall,
            StepName::Install,
            StepName::ForceInstall,
            StepName::Version,
            StepName::Audit,
            StepName::Build,
        ]
    }

    pub const fn default_steps() -> [StepName; 4] {
        [
            StepName::Uninstall,
            StepName::Install,
            StepName::Audit,
            StepName::Build,
        ]
    }

    pub fn label(self) -> &'static str {
        match self {
            StepName::Uninstall => "uninstall",
            StepName::Install => "install",
            StepName::ForceInstall => "force install",
            StepName::Version => "version",
            StepName::Audit => "audit",
            StepName::Build => "build",
        }
    }
}

/// Which part of the project's own version the `version` step raises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum VersionBump {
    #[default]
    Patch,
    Minor,
    Major,
}

impl VersionBump {
    pub fn label(self) -> &'static str {
        match self {
            VersionBump::Patch => "patch",
            VersionBump::Minor => "minor",
            VersionBump::Major => "major",
        }
    }
}

/// How the `version` step treats each project's `package.json` version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct VersionPolicy {
    pub bump: VersionBump,
    /// Only bump when the version still equals the one on the main branch,
    /// so a project that was already raised on this branch is left alone.
    pub only_if_same_as_main: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutionMode {
    #[default]
    PerProject,
    PerStep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    #[default]
    Pending,
    Running,
    Ok,
    Warn,
    Failed,
    Skipped,
}

impl JobStatus {
    pub fn is_final(self) -> bool {
        matches!(self, JobStatus::Failed | JobStatus::Skipped)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub name: String,
    pub directory: PathBuf,
    pub packages: Vec<PackageSpec>,
    pub steps: Vec<StepName>,
    pub install_args: Vec<String>,
    pub audit_fix_args: Vec<String>,
    pub depends_on: Vec<String>,
    pub command: Option<String>,
    pub version: VersionPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnosis {
    pub code: String,
    pub title: String,
    pub hint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyChange {
    pub name: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPackage {
    pub name: String,
    pub expected: String,
    pub installed: Option<String>,
    pub integrity: Option<String>,
    pub resolved: Option<String>,
    pub matches: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunPlan {
    pub project_names: Vec<String>,
    pub packages: Vec<PackageSpec>,
    pub steps: Vec<StepName>,
    pub mode: ExecutionMode,
    pub concurrency: u32,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub extra_install_args: Vec<String>,
    pub label: String,
    #[serde(default)]
    pub save_as: Option<String>,
    #[serde(default)]
    pub version: VersionPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedRun {
    pub name: String,
    pub saved_at: String,
    pub projects: Vec<String>,
    pub packages: Vec<PackageSpec>,
    pub steps: Vec<StepName>,
    pub concurrency: u32,
    #[serde(default)]
    pub mode: ExecutionMode,
    #[serde(default)]
    pub extra_install_args: Vec<String>,
    #[serde(default)]
    pub version: VersionPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepTiming {
    pub step: StepName,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobSnapshot {
    pub name: String,
    pub directory: String,
    pub packages: Vec<String>,
    pub status: JobStatus,
    pub current_step: String,
    pub started_at_ms: Option<u64>,
    pub step_started_at_ms: Option<u64>,
    pub duration_ms: u64,
    pub step_timings: Vec<StepTiming>,
    pub error: String,
    pub warning: String,
    pub depends_on: Vec<String>,
    pub installed: Vec<InstalledPackage>,
    pub diagnosis: Option<Diagnosis>,
    pub dependency_changes: Vec<DependencyChange>,
    pub retries: u32,
    pub backup_available: bool,
    pub log: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSnapshot {
    pub id: String,
    pub label: String,
    pub packages: Vec<String>,
    pub steps: Vec<StepName>,
    pub mode: ExecutionMode,
    pub concurrency: u32,
    pub dry_run: bool,
    pub started_at_ms: u64,
    pub finished_at_ms: Option<u64>,
    pub current_phase: Option<StepName>,
    pub aborted: bool,
    pub command: Option<String>,
    pub jobs: Vec<JobSnapshot>,
    pub summary: Option<RunSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunProjectSummary {
    pub name: String,
    pub directory: String,
    pub status: JobStatus,
    pub duration_ms: u64,
    pub step_timings: Vec<StepTiming>,
    pub error: String,
    pub warning: String,
    #[serde(default)]
    pub installed: Vec<InstalledPackage>,
    #[serde(default)]
    pub diagnosis: Option<Diagnosis>,
    #[serde(default)]
    pub dependency_changes: Vec<DependencyChange>,
    #[serde(default)]
    pub retries: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub label: String,
    pub packages: Vec<String>,
    pub project_count: usize,
    pub concurrency: u32,
    pub steps: Vec<StepName>,
    pub mode: ExecutionMode,
    pub dry_run: bool,
    pub started_at: String,
    pub finished_at: String,
    pub total_duration_ms: u64,
    pub busy_duration_ms: u64,
    pub log_file: Option<String>,
    pub projects: Vec<RunProjectSummary>,
    pub ok_count: usize,
    pub warn_count: usize,
    pub failed_count: usize,
    pub skipped_count: usize,
    pub aborted: bool,
}
