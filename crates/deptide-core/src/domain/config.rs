use serde::{Deserialize, Serialize};

use super::{ConfiguredProject, ExecutionMode, PackageSpec, StepName};

pub const DEFAULT_SCAN_DEPTH: u32 = 6;
pub const DEFAULT_CONCURRENCY: u32 = 3;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateConfig {
    #[serde(default)]
    pub packages: Vec<PackageSpec>,
    #[serde(default)]
    pub projects: Vec<ConfiguredProject>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub install_args: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audit_fix_args: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transfer_ignore: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub projects_root: String,
    pub scan_depth: u32,
    pub concurrency: u32,
    pub steps: Vec<StepName>,
    pub mode: ExecutionMode,
    pub extra_ignored_directories: Vec<String>,
    pub branch_suffix_pattern: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            projects_root: String::new(),
            scan_depth: DEFAULT_SCAN_DEPTH,
            concurrency: DEFAULT_CONCURRENCY,
            steps: StepName::default_steps().to_vec(),
            mode: ExecutionMode::PerProject,
            extra_ignored_directories: Vec::new(),
            branch_suffix_pattern: crate::scan::DEFAULT_BRANCH_SUFFIX_PATTERN.to_string(),
        }
    }
}

impl Settings {
    pub fn sanitized(mut self) -> Self {
        let defaults = Settings::default();

        self.projects_root = self.projects_root.trim().to_string();
        if self.scan_depth == 0 {
            self.scan_depth = defaults.scan_depth;
        }
        if self.concurrency == 0 {
            self.concurrency = defaults.concurrency;
        }
        if self.steps.is_empty() {
            self.steps = defaults.steps;
        }

        let mut names: Vec<String> = Vec::new();
        for name in &self.extra_ignored_directories {
            let trimmed = name.trim();
            if !trimmed.is_empty() && !names.iter().any(|known| known == trimmed) {
                names.push(trimmed.to_string());
            }
        }
        self.extra_ignored_directories = names;

        self.branch_suffix_pattern = self.branch_suffix_pattern.trim().to_string();
        if self.branch_suffix_pattern.is_empty()
            || regex::Regex::new(&self.branch_suffix_pattern).is_err()
        {
            self.branch_suffix_pattern = defaults.branch_suffix_pattern;
        }

        self
    }
}
