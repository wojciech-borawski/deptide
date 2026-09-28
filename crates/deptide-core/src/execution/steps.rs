use crate::domain::{BumpWhen, DependencySection, Job, PackageSpec, StepName, VersionBump};
use crate::scan::BUILD_SCRIPT_NAME;
use crate::util::text::describe_count;

const SAVE_EXACT: &str = "--save-exact";
const SAVE_EXACT_FLAGS: &[&str] = &[SAVE_EXACT, "-E", "--save-exact=true"];
const NO_SAVE_EXACT: &str = "--no-save-exact";

fn is_save_exact(arg: &str) -> bool {
    SAVE_EXACT_FLAGS.contains(&arg)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestEntry {
    pub section: DependencySection,
    pub name: String,
    pub range: String,
}

impl ManifestEntry {
    pub(super) fn field(&self) -> &'static str {
        match self.section {
            DependencySection::Dependencies => "dependencies",
            DependencySection::Dev => "devDependencies",
            DependencySection::Peer => "peerDependencies",
        }
    }

    pub fn describe(&self) -> String {
        format!("{}.{} = {}", self.field(), self.name, self.range)
    }
}

pub fn describe_entries(entries: &[ManifestEntry]) -> String {
    entries
        .iter()
        .map(ManifestEntry::describe)
        .collect::<Vec<_>>()
        .join(", ")
}

pub struct PlannedCommand {
    pub label: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallGroup {
    Normal,
    Dev,
    Peer,
}

impl InstallGroup {
    pub fn of(spec: &PackageSpec) -> Self {
        if spec.save_dev {
            InstallGroup::Dev
        } else if spec.save_peer {
            InstallGroup::Peer
        } else {
            InstallGroup::Normal
        }
    }

    fn flag(self) -> Option<&'static str> {
        match self {
            InstallGroup::Normal => None,
            InstallGroup::Dev => Some("--save-dev"),
            InstallGroup::Peer => Some("--save-peer"),
        }
    }
}

pub fn group_by_install(packages: &[PackageSpec]) -> Vec<(InstallGroup, Vec<String>)> {
    [InstallGroup::Normal, InstallGroup::Dev, InstallGroup::Peer]
        .into_iter()
        .map(|group| {
            let specs: Vec<String> = packages
                .iter()
                .filter(|spec| InstallGroup::of(spec) == group)
                .map(PackageSpec::to_spec)
                .collect();
            (group, specs)
        })
        .filter(|(_, specs)| !specs.is_empty())
        .collect()
}

pub fn save_exact_peer_note(job: &Job) -> Option<String> {
    let has_peer = job
        .packages
        .iter()
        .any(|spec| InstallGroup::of(spec) == InstallGroup::Peer);
    let has_exact = job.install_args.iter().any(|arg| is_save_exact(arg));

    (has_peer && has_exact).then(|| {
        format!("{SAVE_EXACT} dropped for peer installs: an exact peer range pins consumers to one version")
    })
}

pub fn plan_uninstall(job: &Job) -> PlannedCommand {
    let names: Vec<String> = job.packages.iter().map(|spec| spec.name.clone()).collect();
    let label = format!(
        "uninstall {}",
        describe_count(names.len(), names.first().map(String::as_str), "packages")
    );

    let mut args = vec!["uninstall".to_string()];
    args.extend(names);
    PlannedCommand { label, args }
}

pub fn plan_install(job: &Job, force: bool) -> Vec<PlannedCommand> {
    group_by_install(&job.packages)
        .into_iter()
        .map(|(group, specs)| {
            let label = format!(
                "{} {}",
                if force { "force install" } else { "install" },
                describe_count(specs.len(), specs.first().map(String::as_str), "packages")
            );

            let mut args = vec!["install".to_string()];
            args.extend(specs);
            if let Some(flag) = group.flag() {
                args.push(flag.to_string());
            }
            if force && !job.install_args.iter().any(|arg| arg == "--force") {
                args.push("--force".to_string());
            }
            args.extend(
                job.install_args
                    .iter()
                    .filter(|arg| group != InstallGroup::Peer || !is_save_exact(arg))
                    .cloned(),
            );
            if group == InstallGroup::Peer {
                args.push(NO_SAVE_EXACT.to_string());
            }
            PlannedCommand { label, args }
        })
        .collect()
}

pub fn plan_audit_fix(job: &Job) -> PlannedCommand {
    let mut args = vec!["audit".to_string(), "fix".to_string()];
    args.extend(job.audit_fix_args.iter().cloned());
    PlannedCommand {
        label: "audit fix".to_string(),
        args,
    }
}

pub fn plan_build() -> PlannedCommand {
    PlannedCommand {
        label: "build".to_string(),
        args: vec!["run".to_string(), BUILD_SCRIPT_NAME.to_string()],
    }
}

pub fn plan_version_bump(bump: VersionBump) -> PlannedCommand {
    PlannedCommand {
        label: format!("version {}", bump.label()),
        args: vec![
            "version".to_string(),
            bump.label().to_string(),
            "--no-git-tag-version".to_string(),
        ],
    }
}

pub fn dry_run_lines(job: &Job, step: StepName) -> Vec<String> {
    match step {
        StepName::Uninstall => {
            let names: Vec<&str> = job.packages.iter().map(|spec| spec.name.as_str()).collect();
            vec![format!("would uninstall {}", names.join(" "))]
        }
        StepName::Install | StepName::ForceInstall => {
            let force = if step == StepName::ForceInstall {
                " --force"
            } else {
                ""
            };
            let mut lines: Vec<String> = save_exact_peer_note(job).into_iter().collect();
            lines.extend(job.packages.iter().map(|spec| {
                let flag = InstallGroup::of(spec)
                    .flag()
                    .map(|flag| format!(" {flag}"))
                    .unwrap_or_default();
                format!("would install {}{flag}{force}", spec.to_spec())
            }));
            lines
        }
        StepName::Version => {
            let condition = match job.version.when {
                BumpWhen::Always => "",
                BumpWhen::SameAsMain => " if it still equals the version on main",
                BumpWhen::NotBumpedOnBranch => " unless this branch already changed it",
            };
            vec![format!(
                "would bump the {} version{condition}",
                job.version.bump.label()
            )]
        }
        StepName::Audit => vec!["would run npm audit fix".to_string()],
        StepName::Build => vec!["would run npm run build".to_string()],
    }
}
