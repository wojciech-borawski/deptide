use std::path::Path;

use super::pack::{is_packed_without_saving, plan_pack, tarball_path, uninstall_note};
use crate::domain::{BumpWhen, DependencySection, Job, PackageSpec, Reason, StepName, VersionBump};
use crate::scan::{read_project_at, BUILD_SCRIPT_NAME};
use crate::util::text::describe_count;

const SAVE_EXACT: &str = "--save-exact";
const SAVE_EXACT_FLAGS: &[&str] = &[SAVE_EXACT, "-E", "--save-exact=true"];
const NO_SAVE_EXACT: &str = "--no-save-exact";
const NO_SAVE: &str = "--no-save";
const SHELL_SPECIAL: &[char] = &[
    '^', '&', '|', '<', '>', '(', ')', '%', '!', '"', '\'', ';', ',', '$', '`', '{', '}',
];

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

fn quote_arg(arg: &str) -> String {
    let needs_quotes = arg.is_empty()
        || arg
            .chars()
            .any(|character| character.is_whitespace() || SHELL_SPECIAL.contains(&character));
    if needs_quotes {
        format!("\"{}\"", arg.replace('"', "\\\""))
    } else {
        arg.to_string()
    }
}

impl PlannedCommand {
    pub fn command_line(&self) -> String {
        std::iter::once("npm".to_string())
            .chain(self.args.iter().map(|arg| quote_arg(arg)))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

pub fn has_build_script(directory: &Path) -> bool {
    read_project_at(directory, directory)
        .map(|project| project.has_build_script)
        .unwrap_or(false)
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

fn install_argument(job: &Job, spec: &PackageSpec) -> Option<String> {
    match spec.pack {
        Some(_) => tarball_path(job, &spec.name).map(|path| path.to_string_lossy().to_string()),
        None => Some(spec.to_spec()),
    }
}

fn group_by_install(job: &Job, packages: &[PackageSpec]) -> Vec<(InstallGroup, Vec<String>)> {
    [InstallGroup::Normal, InstallGroup::Dev, InstallGroup::Peer]
        .into_iter()
        .map(|group| {
            let specs: Vec<String> = packages
                .iter()
                .filter(|spec| InstallGroup::of(spec) == group)
                .filter_map(|spec| install_argument(job, spec))
                .collect();
            (group, specs)
        })
        .filter(|(_, specs)| !specs.is_empty())
        .collect()
}

pub fn save_exact_peer_note(job: &Job) -> Option<Reason> {
    let has_peer = job
        .packages
        .iter()
        .any(|spec| InstallGroup::of(spec) == InstallGroup::Peer);
    let has_exact = job.install_args.iter().any(|arg| is_save_exact(arg));

    (has_peer && has_exact).then_some(Reason::SaveExactDroppedForPeers {})
}

pub fn plan_uninstall(job: &Job) -> Option<PlannedCommand> {
    let names: Vec<String> = job
        .packages
        .iter()
        .filter(|spec| !is_packed_without_saving(spec))
        .map(|spec| spec.name.clone())
        .collect();
    if names.is_empty() {
        return None;
    }
    let label = format!(
        "uninstall {}",
        describe_count(names.len(), names.first().map(String::as_str), "packages")
    );

    let mut args = vec!["uninstall".to_string()];
    args.extend(names);
    Some(PlannedCommand { label, args })
}

pub fn plan_install(job: &Job, force: bool) -> Vec<PlannedCommand> {
    let (unsaved, registry): (Vec<PackageSpec>, Vec<PackageSpec>) = job
        .packages
        .iter()
        .cloned()
        .partition(is_packed_without_saving);
    let mut commands: Vec<PlannedCommand> = group_by_install(job, &registry)
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
        .collect();
    commands.extend(plan_unsaved_install(job, &unsaved, force));
    commands
}

fn plan_unsaved_install(job: &Job, unsaved: &[PackageSpec], force: bool) -> Option<PlannedCommand> {
    let tarballs: Vec<String> = unsaved
        .iter()
        .filter_map(|spec| install_argument(job, spec))
        .collect();
    if tarballs.is_empty() {
        return None;
    }
    let first = unsaved.first()?;
    let label = format!(
        "{} {} from its tarball",
        if force { "force install" } else { "install" },
        describe_count(unsaved.len(), Some(&first.name), "packages")
    );

    let mut args = vec!["install".to_string()];
    args.extend(tarballs);
    if force && !job.install_args.iter().any(|arg| arg == "--force") {
        args.push("--force".to_string());
    }
    args.extend(job.install_args.iter().cloned());
    args.push(NO_SAVE.to_string());
    Some(PlannedCommand { label, args })
}

pub fn plan_audit_fix(job: &Job) -> PlannedCommand {
    let mut args = vec!["audit".to_string(), "fix".to_string()];
    args.extend(job.audit_fix_args.iter().cloned());
    PlannedCommand {
        label: "audit fix".to_string(),
        args,
    }
}

pub fn plan_reinstall_after_audit_fix(job: &Job) -> Option<PlannedCommand> {
    let installs = |wanted: StepName| job.steps.contains(&wanted);
    if !installs(StepName::Install) && !installs(StepName::ForceInstall) {
        return None;
    }
    let unsaved: Vec<PackageSpec> = job
        .packages
        .iter()
        .filter(|spec| is_packed_without_saving(spec))
        .cloned()
        .collect();
    plan_unsaved_install(job, &unsaved, installs(StepName::ForceInstall))
}

pub fn plan_audit_report() -> PlannedCommand {
    PlannedCommand {
        label: "audit report".to_string(),
        args: vec!["audit".to_string(), "--json".to_string()],
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

fn would_run(command: &PlannedCommand) -> String {
    format!("would run {}", command.command_line())
}

pub fn dry_run_lines(job: &Job, step: StepName) -> Vec<String> {
    match step {
        StepName::Uninstall => {
            let mut lines: Vec<String> = uninstall_note(job)
                .map(|reason| reason.text())
                .into_iter()
                .collect();
            let names: Vec<&str> = job
                .packages
                .iter()
                .filter(|spec| !is_packed_without_saving(spec))
                .map(|spec| spec.name.as_str())
                .collect();
            if !names.is_empty() {
                lines.push(format!("would uninstall {}", names.join(" ")));
            }
            lines
        }
        StepName::Install | StepName::ForceInstall => {
            let mut lines: Vec<String> = save_exact_peer_note(job)
                .map(|reason| reason.text())
                .into_iter()
                .collect();
            lines.extend(
                plan_install(job, step == StepName::ForceInstall)
                    .iter()
                    .map(would_run),
            );
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
        StepName::Audit => std::iter::once(plan_audit_fix(job))
            .chain(plan_reinstall_after_audit_fix(job))
            .map(|command| would_run(&command))
            .collect(),
        StepName::AuditReport => vec!["would run npm audit --json".to_string()],
        StepName::Build => vec!["would run npm run build".to_string()],
        StepName::Pack => match &job.packing {
            Some(packing) => {
                let pack = plan_pack(job, packing);
                let notes: Vec<String> = pack.notes.iter().map(Reason::text).collect();
                notes
                    .into_iter()
                    .chain(pack.commands().iter().map(would_run))
                    .collect()
            }
            None => Vec::new(),
        },
    }
}
