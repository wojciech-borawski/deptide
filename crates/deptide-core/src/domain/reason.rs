use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "params", rename_all = "camelCase")]
pub enum Reason {
    StillEqualsMain {
        version: Option<String>,
    },
    DiffersFromMain {
        version: Option<String>,
        main: String,
    },
    MainUnreadable {},
    NotBumpedOnBranch {
        version: Option<String>,
        base: String,
        commit: String,
    },
    ChangedUncommitted {
        version: Option<String>,
        committed: String,
    },
    ChangedOnBranch {
        version: Option<String>,
        from: String,
        to: String,
        commit: String,
    },
    BranchUnknown {
        cause: LookupCause,
    },
    LookupStopped {},
    InvalidVersion {
        version: Option<String>,
    },
    ManifestUnreadable {},
    NoBuildScript {},
    SaveExactDroppedForPeers {},
    NoLibraries {},
    NoProjectUsesPackages {},
    PackedNotUninstalled {
        names: String,
    },
    BuiltBeforePacking {},
    PackedLibraryMissing {
        name: String,
    },
    PackedLibraryAmbiguous {
        name: String,
        projects: String,
    },
    OnlyPacked {},
    PackedBeforeOwnChanges {},
}

fn shown(version: &Option<String>) -> &str {
    version.as_deref().unwrap_or("none")
}

impl Reason {
    pub fn text(&self) -> String {
        match self {
            Reason::StillEqualsMain { version } => {
                format!("version {} still equals main", shown(version))
            }
            Reason::DiffersFromMain { version, main } => {
                format!("version {} already differs from main ({main})", shown(version))
            }
            Reason::MainUnreadable {} => "could not read the version on the main branch".to_string(),
            Reason::NotBumpedOnBranch {
                version,
                base,
                commit,
            } => format!(
                "no commit of this branch changed version {} since {base} ({commit})",
                shown(version)
            ),
            Reason::ChangedUncommitted { version, committed } => format!(
                "version {} already changed on this branch (uncommitted, {committed} in the last commit)",
                shown(version)
            ),
            Reason::ChangedOnBranch {
                version,
                from,
                to,
                commit,
            } => format!(
                "version {} already changed on this branch ({from} -> {to} in {commit})",
                shown(version)
            ),
            Reason::BranchUnknown { cause } => format!(
                "cannot tell whether this branch bumped the version: {}",
                cause.text()
            ),
            Reason::LookupStopped {} => "the git lookup stopped".to_string(),
            Reason::InvalidVersion { version } => {
                format!("npm version cannot raise {}", shown(version))
            }
            Reason::ManifestUnreadable {} => "package.json cannot be read".to_string(),
            Reason::NoBuildScript {} => "no build script, step skipped".to_string(),
            Reason::SaveExactDroppedForPeers {} => {
                "--save-exact dropped for peer installs: an exact peer range pins consumers to one version"
                    .to_string()
            }
            Reason::NoLibraries {} => {
                "Nothing to do: without libraries only the version, audit, audit report and build steps can run"
                    .to_string()
            }
            Reason::NoProjectUsesPackages {} => {
                "Nothing to do: none of the selected projects uses the chosen packages".to_string()
            }
            Reason::PackedNotUninstalled { names } => {
                format!("{names} not uninstalled: installed from a packed tarball without saving")
            }
            Reason::BuiltBeforePacking {} => {
                "already built by its build step, packed without building again".to_string()
            }
            Reason::PackedLibraryMissing { name } => format!(
                "{name} cannot be packed: no configured project has that name in its package.json"
            ),
            Reason::PackedBeforeOwnChanges {} => "packed before its own install phase, so the tarball does not hold this run's changes to it".to_string(),
            Reason::PackedLibraryAmbiguous { name, projects } => format!(
                "{name} cannot be packed: more than one configured project has that name in its package.json ({projects})"
            ),
            Reason::OnlyPacked {} => "only packed: none of the chosen libraries applies to this project, so its other steps do not run".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LookupCause {
    NotARepository,
    NoCommits,
    DetachedHead,
    OnMainBranch,
    NoReflog,
    NoOtherBranch,
    BranchListFailed,
    RemoteListFailed,
    CommitListFailed,
    ShallowClone,
    NoSharedHistory,
    NoVersionAtBase,
    NoVersionInLastCommit,
    HistoryUnreadable,
}

impl LookupCause {
    pub fn text(self) -> &'static str {
        match self {
            LookupCause::NotARepository => {
                "the folder is not a git repository or git is not installed"
            }
            LookupCause::NoCommits => "the repository has no commits yet",
            LookupCause::DetachedHead => "HEAD is detached, check out a branch",
            LookupCause::OnMainBranch => "on the main branch, no base branch to compare with",
            LookupCause::NoReflog => "cannot tell when this branch was created, it has no reflog",
            LookupCause::NoOtherBranch => "no other branch to compare with",
            LookupCause::BranchListFailed => "could not list the branches",
            LookupCause::RemoteListFailed => "could not list the remotes",
            LookupCause::CommitListFailed => "could not list the commits of this branch",
            LookupCause::ShallowClone => "the clone is shallow, fetch the full history",
            LookupCause::NoSharedHistory => "this branch shares no history with any other branch",
            LookupCause::NoVersionAtBase => "package.json has no version at the base commit",
            LookupCause::NoVersionInLastCommit => "package.json has no version in the last commit",
            LookupCause::HistoryUnreadable => "could not read package.json from the history",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReasonPreview {
    #[serde(flatten)]
    pub reason: Reason,
    pub text: String,
}

impl From<Reason> for ReasonPreview {
    fn from(reason: Reason) -> Self {
        let text = reason.text();
        Self { reason, text }
    }
}
