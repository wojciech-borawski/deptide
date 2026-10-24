use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{PackageSpec, ReasonPreview};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StepName {
    Uninstall,
    Install,
    #[serde(rename = "force-install")]
    ForceInstall,
    Version,
    Audit,
    #[serde(rename = "audit-report")]
    AuditReport,
    Build,
    Pack,
}

impl StepName {
    pub const fn all() -> [StepName; 7] {
        [
            StepName::Uninstall,
            StepName::Install,
            StepName::ForceInstall,
            StepName::Version,
            StepName::Audit,
            StepName::AuditReport,
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

    /// Whether the step adds or removes the run's packages, so it needs at least one.
    pub fn changes_packages(self) -> bool {
        matches!(
            self,
            StepName::Uninstall | StepName::Install | StepName::ForceInstall
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            StepName::Uninstall => "uninstall",
            StepName::Install => "install",
            StepName::ForceInstall => "force install",
            StepName::Version => "version",
            StepName::Audit => "audit",
            StepName::AuditReport => "audit report",
            StepName::Build => "build",
            StepName::Pack => "pack",
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

/// When the `version` step bumps a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum BumpWhen {
    #[default]
    Always,
    /// Only while the version still equals the one on the main branch.
    SameAsMain,
    /// Only while the current branch's own commits have not changed the version.
    NotBumpedOnBranch,
}

/// How the `version` step treats each project's `package.json` version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", from = "StoredVersionPolicy")]
pub struct VersionPolicy {
    pub bump: VersionBump,
    pub when: BumpWhen,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct StoredVersionPolicy {
    bump: VersionBump,
    when: Option<BumpWhen>,
    only_if_same_as_main: bool,
}

impl From<StoredVersionPolicy> for VersionPolicy {
    fn from(stored: StoredVersionPolicy) -> Self {
        let legacy = if stored.only_if_same_as_main {
            BumpWhen::SameAsMain
        } else {
            BumpWhen::Always
        };

        Self {
            bump: stored.bump,
            when: stored.when.unwrap_or(legacy),
        }
    }
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
    pub packing: Option<Packing>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packing {
    pub folder: PathBuf,
    pub libraries: BTreeMap<String, PackedLibrary>,
    pub ambiguous: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackedLibrary {
    pub directory: PathBuf,
    pub tarball: Option<PackedTarball>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackedTarball {
    pub file: String,
    pub version: String,
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

/// Vulnerability counts per severity, as `npm audit --json` reports them in `metadata.vulnerabilities`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct AuditCounts {
    pub info: u32,
    pub low: u32,
    pub moderate: u32,
    pub high: u32,
    pub critical: u32,
    pub total: u32,
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
    /// Bump levels that differ from `version.bump`, by project name.
    #[serde(default)]
    pub project_bumps: BTreeMap<String, VersionBump>,
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
    /// Bump levels that differ from `version.bump`, by project name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub project_bumps: BTreeMap<String, VersionBump>,
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
    pub audit: Option<AuditCounts>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit: Option<AuditCounts>,
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
    /// The run's folder under `backups/`; absent for dry runs and for summaries written before it was recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup_folder: Option<String>,
}

/// The backups a past run left behind, found from its summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunBackup {
    /// The summary's file name without extension, which names the run for `restore`.
    pub run: String,
    /// The summary's `logFile`, so a screen can match the backup to the summary it shows.
    pub log_file: String,
    pub folder: String,
    pub projects: Vec<BackedUpProject>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackedUpProject {
    pub name: String,
    pub directory: String,
    /// The backed up file names that a restore writes back.
    pub files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRestore {
    pub name: String,
    #[serde(flatten)]
    pub outcome: RestoreOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "camelCase")]
pub enum RestoreOutcome {
    Restored { files: Vec<String> },
    Failed { error: String },
}

/// What a run of a plan would do, worked out before it starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanPreview {
    /// Jobs in the order the run starts them.
    pub jobs: Vec<JobPreview>,
    /// Selected project names that are not in the configuration.
    pub missing: Vec<String>,
    /// Selected projects that use none of the plan's packages, so they get no job.
    pub without_packages: Vec<String>,
    /// The error starting the run would fail with, when no job is left.
    pub nothing_to_do: Option<ReasonPreview>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobPreview {
    pub name: String,
    pub depends_on: Vec<String>,
    pub steps: Vec<StepPreview>,
    /// Present when the job runs the version step.
    pub version: Option<VersionPreview>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepPreview {
    pub step: StepName,
    /// The npm command lines the step runs, in order, as the run logs them after `$ `.
    pub commands: Vec<String>,
    /// What the run logs for the step besides its commands; `text` is the logged line.
    pub notes: Vec<ReasonPreview>,
}

/// What the version step will do to one project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "camelCase")]
pub enum VersionPreview {
    #[serde(rename_all = "camelCase")]
    Bump {
        /// `None` when package.json has no version; npm then starts from 0.0.0.
        current: Option<String>,
        next: String,
        level: VersionBump,
        reason: Option<ReasonPreview>,
    },
    #[serde(rename_all = "camelCase")]
    Skip {
        current: Option<String>,
        level: VersionBump,
        reason: ReasonPreview,
    },
    /// The step would run npm, but npm cannot bump the current version.
    #[serde(rename_all = "camelCase")]
    Fail {
        current: Option<String>,
        level: VersionBump,
        reason: ReasonPreview,
    },
}

impl VersionPreview {
    pub fn version_after(&self) -> Option<&str> {
        match self {
            VersionPreview::Bump { next, .. } => Some(next),
            VersionPreview::Skip { current, .. } => current.as_deref(),
            VersionPreview::Fail { .. } => None,
        }
    }
}
