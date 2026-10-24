use std::path::Path;

use serde_json::Value;

use crate::domain::{BumpWhen, Job, Reason, ReasonPreview, VersionBump, VersionPreview};
use crate::scan::{
    read_package_manifest, version_change_on_branch, version_on_main_branch, BranchVersionChange,
    MANIFEST_FILE_NAME,
};
use crate::util::json::read_json;
use crate::util::semver::Version;

/// The version `npm version <bump>` writes for `current`, or `None` when npm would reject it.
pub fn next_version(current: &str, bump: VersionBump) -> Option<String> {
    let version = Version::clean(current)?;
    let (major, minor, patch) = (version.major, version.minor, version.patch);
    let prerelease = version.is_prerelease();

    let (major, minor, patch) = match bump {
        VersionBump::Patch if prerelease => (major, minor, patch),
        VersionBump::Patch => (major, minor, patch.checked_add(1)?),
        VersionBump::Minor if prerelease && patch == 0 => (major, minor, 0),
        VersionBump::Minor => (major, minor.checked_add(1)?, 0),
        VersionBump::Major if prerelease && minor == 0 && patch == 0 => (major, 0, 0),
        VersionBump::Major => (major.checked_add(1)?, 0, 0),
    };

    Some(format!("{major}.{minor}.{patch}"))
}

/// The version `npm version <bump>` writes for a package.json whose `version` field is `field`,
/// or `None` when npm fails. Like npm, a missing or empty version counts as `0.0.0`.
pub fn next_manifest_version(field: Option<&Value>, bump: VersionBump) -> Option<String> {
    match field {
        None | Some(Value::Null) | Some(Value::Bool(false)) => next_version("0.0.0", bump),
        Some(Value::String(text)) if text.is_empty() => next_version("0.0.0", bump),
        Some(Value::Number(number)) if number.as_f64() == Some(0.0) => next_version("0.0.0", bump),
        Some(Value::String(text)) => next_version(text, bump),
        Some(_) => None,
    }
}

/// Whether the `version` step bumps a project, with the reason the run logs when git decided it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BumpDecision {
    Bump { reason: Option<Reason> },
    Skip { reason: Reason },
}

impl BumpDecision {
    /// The line the run logs for the decision, if any.
    pub fn log_line(&self) -> Option<String> {
        match self {
            BumpDecision::Bump { reason } => reason
                .as_ref()
                .map(|reason| format!("{}, bumping", reason.text())),
            BumpDecision::Skip { reason } => Some(format!("{}, bump skipped", reason.text())),
        }
    }
}

/// Decides the `version` step for the project in `directory` whose working-tree version is
/// `current`. Runs git, so it blocks.
pub fn decide_bump(directory: &Path, when: BumpWhen, current: Option<&str>) -> BumpDecision {
    let shown = current.map(str::to_string);

    match when {
        BumpWhen::Always => BumpDecision::Bump { reason: None },
        BumpWhen::SameAsMain => match version_on_main_branch(directory) {
            Some(main) if current == Some(main.as_str()) => BumpDecision::Bump {
                reason: Some(Reason::StillEqualsMain { version: shown }),
            },
            Some(main) => BumpDecision::Skip {
                reason: Reason::DiffersFromMain {
                    version: shown,
                    main,
                },
            },
            None => BumpDecision::Skip {
                reason: Reason::MainUnreadable {},
            },
        },
        BumpWhen::NotBumpedOnBranch => match version_change_on_branch(directory) {
            BranchVersionChange::Unchanged {
                version,
                base_branch,
                commit,
            } if current == Some(version.as_str()) => BumpDecision::Bump {
                reason: Some(Reason::NotBumpedOnBranch {
                    version: shown,
                    base: base_branch,
                    commit,
                }),
            },
            BranchVersionChange::Unchanged { version, .. } => BumpDecision::Skip {
                reason: Reason::ChangedUncommitted {
                    version: shown,
                    committed: version,
                },
            },
            BranchVersionChange::Changed { from, to, commit } => BumpDecision::Skip {
                reason: Reason::ChangedOnBranch {
                    version: shown,
                    from,
                    to,
                    commit,
                },
            },
            BranchVersionChange::Unavailable(cause) => BumpDecision::Skip {
                reason: Reason::BranchUnknown { cause },
            },
        },
    }
}

pub fn preview_version_step(job: &Job) -> (VersionPreview, BumpDecision) {
    let current = read_package_manifest(&job.directory).and_then(|manifest| manifest.version);
    let level = job.version.bump;
    let decision = decide_bump(&job.directory, job.version.when, current.as_deref());

    let preview = match &decision {
        BumpDecision::Skip { reason } => VersionPreview::Skip {
            current,
            level,
            reason: reason.clone().into(),
        },
        BumpDecision::Bump { reason } => {
            let manifest = read_json::<Value>(&job.directory.join(MANIFEST_FILE_NAME));
            let next = match manifest {
                Ok(Some(manifest)) => next_manifest_version(manifest.get("version"), level)
                    .ok_or_else(|| Reason::InvalidVersion {
                        version: current.clone(),
                    }),
                _ => Err(Reason::ManifestUnreadable {}),
            };
            match next {
                Ok(next) => VersionPreview::Bump {
                    current,
                    next,
                    level,
                    reason: reason.clone().map(ReasonPreview::from),
                },
                Err(reason) => VersionPreview::Fail {
                    current,
                    level,
                    reason: reason.into(),
                },
            }
        }
    };
    (preview, decision)
}
