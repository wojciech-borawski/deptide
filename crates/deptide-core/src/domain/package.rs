use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageSpec {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub save_dev: bool,
    /// Installed with `--save-peer`; `save_dev` wins when both are set.
    #[serde(default)]
    pub save_peer: bool,
}

impl PackageSpec {
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            save_dev: false,
            save_peer: false,
        }
    }

    pub fn parse(spec: &str) -> Option<Self> {
        let separator = spec.rfind('@')?;
        if separator == 0 {
            return None;
        }

        let name = spec[..separator].trim();
        let version = spec[separator + 1..].trim();
        if name.is_empty() || version.is_empty() {
            return None;
        }

        Some(Self::new(name, version))
    }

    pub fn to_spec(&self) -> String {
        format!("{}@{}", self.name, self.version)
    }
}

/// A `package.json` dependency field. The order is the wizard's grouping
/// precedence: `dependencies`, then `peerDependencies`, then `devDependencies`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DependencySection {
    Dependencies,
    Peer,
    Dev,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyCandidate {
    pub name: String,
    pub local_version: Option<String>,
    pub local_branch: Option<String>,
    pub branch_suffix: Option<String>,
    pub current_ranges: Vec<String>,
    pub used_by: Vec<String>,
    /// True when every consumer that declares the package has it in
    /// `devDependencies` and not in `dependencies`.
    pub is_dev_dependency: bool,
    /// Every section the package appears in across the consumers, in
    /// precedence order, without duplicates.
    pub sections: Vec<DependencySection>,
}
