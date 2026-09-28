use clap::{Args, Parser, Subcommand, ValueEnum};

use deptide_core::domain::{BumpWhen, ExecutionMode, StepName, VersionBump};

#[derive(Parser)]
#[command(
    name = "deptide-cli",
    version,
    about = "Runs Deptide workspaces from the command line: scan projects, update libraries, run commands."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    #[command(about = "Find npm projects under the configured projects root")]
    Scan(ScanArgs),
    #[command(
        about = "Update libraries in the selected projects, the same way the desktop app does"
    )]
    Run(RunArgs),
    #[command(about = "Run one shell command in several projects at once")]
    Exec(ExecArgs),
    #[command(about = "List past runs recorded in the workspace")]
    History(HistoryArgs),
}

#[derive(Args)]
pub struct ScanArgs {
    #[arg(help = "Workspace folder holding update-libs.json")]
    pub workspace: String,
    #[arg(
        long,
        help = "Add every found project to the configuration instead of only listing them"
    )]
    pub apply: bool,
}

#[derive(Args)]
pub struct RunArgs {
    #[arg(help = "Workspace folder holding update-libs.json")]
    pub workspace: String,
    #[arg(
        short,
        long = "package",
        value_name = "NAME@VERSION",
        required = true,
        help = "Library to install, repeatable"
    )]
    pub packages: Vec<String>,
    #[arg(
        short = 'P',
        long = "project",
        value_name = "NAME",
        help = "Project to touch, repeatable; defaults to every configured project that is not skipped"
    )]
    pub projects: Vec<String>,
    #[arg(
        long,
        value_delimiter = ',',
        value_enum,
        help = "Steps to run in order; defaults to the workspace settings"
    )]
    pub steps: Option<Vec<StepArg>>,
    #[arg(
        long,
        value_enum,
        help = "Order of work; defaults to the workspace settings"
    )]
    pub mode: Option<ModeArg>,
    #[arg(
        long,
        help = "Projects running at the same time; defaults to the workspace settings"
    )]
    pub concurrency: Option<u32>,
    #[arg(long, help = "Log what would happen without touching any project")]
    pub dry_run: bool,
    #[arg(
        long,
        value_enum,
        default_value = "patch",
        help = "Which part of each project's own version the version step raises"
    )]
    pub bump: BumpArg,
    #[arg(
        long,
        value_enum,
        value_name = "WHEN",
        conflicts_with = "bump_only_if_same_as_main",
        help = "When the version step bumps a project: always (the default), same-as-main (only while its version equals the one on the main branch) or not-bumped-on-branch (only while this branch's own commits have not changed it)"
    )]
    pub bump_when: Option<BumpWhenArg>,
    #[arg(
        long,
        help = "Same as --bump-when same-as-main, kept for older scripts; cannot be combined with --bump-when"
    )]
    pub bump_only_if_same_as_main: bool,
    #[arg(long, help = "Label for the transcript and the history entry")]
    pub label: Option<String>,
    #[arg(
        long = "install-arg",
        value_name = "ARG",
        help = "Extra argument for every npm install, repeatable"
    )]
    pub install_args: Vec<String>,
    #[arg(short, long, help = "Print only status changes, not the npm output")]
    pub quiet: bool,
    #[arg(long, help = "Print the run summary as JSON when the run ends")]
    pub json: bool,
}

#[derive(Args)]
pub struct ExecArgs {
    #[arg(help = "Workspace folder holding update-libs.json")]
    pub workspace: String,
    #[arg(
        short = 'P',
        long = "project",
        value_name = "NAME",
        help = "Project to run in, repeatable; defaults to every configured project that is not skipped"
    )]
    pub projects: Vec<String>,
    #[arg(long, default_value_t = 3, help = "Projects running at the same time")]
    pub concurrency: u32,
    #[arg(
        short,
        long,
        help = "Print only status changes, not the command output"
    )]
    pub quiet: bool,
    #[arg(long, help = "Print the run summary as JSON when the run ends")]
    pub json: bool,
    #[arg(
        required = true,
        trailing_var_arg = true,
        value_name = "COMMAND",
        help = "The shell line to run in every project"
    )]
    pub command: Vec<String>,
}

#[derive(Args)]
pub struct HistoryArgs {
    #[arg(help = "Workspace folder holding update-libs.json")]
    pub workspace: String,
    #[arg(
        long,
        default_value_t = 20,
        help = "How many runs to list, newest first"
    )]
    pub limit: usize,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum StepArg {
    Uninstall,
    Install,
    #[value(name = "force-install")]
    ForceInstall,
    Version,
    Audit,
    Build,
}

impl From<StepArg> for StepName {
    fn from(step: StepArg) -> Self {
        match step {
            StepArg::Uninstall => StepName::Uninstall,
            StepArg::Install => StepName::Install,
            StepArg::ForceInstall => StepName::ForceInstall,
            StepArg::Version => StepName::Version,
            StepArg::Audit => StepName::Audit,
            StepArg::Build => StepName::Build,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
pub enum BumpArg {
    Patch,
    Minor,
    Major,
}

impl From<BumpArg> for VersionBump {
    fn from(bump: BumpArg) -> Self {
        match bump {
            BumpArg::Patch => VersionBump::Patch,
            BumpArg::Minor => VersionBump::Minor,
            BumpArg::Major => VersionBump::Major,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
pub enum BumpWhenArg {
    Always,
    #[value(name = "same-as-main")]
    SameAsMain,
    #[value(name = "not-bumped-on-branch")]
    NotBumpedOnBranch,
}

impl From<BumpWhenArg> for BumpWhen {
    fn from(when: BumpWhenArg) -> Self {
        match when {
            BumpWhenArg::Always => BumpWhen::Always,
            BumpWhenArg::SameAsMain => BumpWhen::SameAsMain,
            BumpWhenArg::NotBumpedOnBranch => BumpWhen::NotBumpedOnBranch,
        }
    }
}

impl RunArgs {
    pub fn bump_when(&self) -> BumpWhen {
        match self.bump_when {
            Some(when) => when.into(),
            None if self.bump_only_if_same_as_main => BumpWhen::SameAsMain,
            None => BumpWhen::Always,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
pub enum ModeArg {
    #[value(name = "per-project")]
    PerProject,
    #[value(name = "per-step")]
    PerStep,
}

impl From<ModeArg> for ExecutionMode {
    fn from(mode: ModeArg) -> Self {
        match mode {
            ModeArg::PerProject => ExecutionMode::PerProject,
            ModeArg::PerStep => ExecutionMode::PerStep,
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::error::ErrorKind;
    use clap::Parser;
    use deptide_core::domain::BumpWhen;

    use super::{Cli, Command};

    fn bump_when(flags: &[&str]) -> Result<BumpWhen, clap::Error> {
        let mut args = vec!["deptide-cli", "run", "workspace", "-p", "left-pad@1.3.0"];
        args.extend_from_slice(flags);
        match Cli::try_parse_from(args)?.command {
            Command::Run(run) => Ok(run.bump_when()),
            _ => panic!("expected the run command"),
        }
    }

    #[test]
    fn bump_when_defaults_to_always_and_accepts_every_condition() {
        assert_eq!(bump_when(&[]).unwrap(), BumpWhen::Always);
        assert_eq!(
            bump_when(&["--bump-when", "always"]).unwrap(),
            BumpWhen::Always
        );
        assert_eq!(
            bump_when(&["--bump-when", "same-as-main"]).unwrap(),
            BumpWhen::SameAsMain
        );
        assert_eq!(
            bump_when(&["--bump-when", "not-bumped-on-branch"]).unwrap(),
            BumpWhen::NotBumpedOnBranch
        );
    }

    #[test]
    fn the_old_flag_means_same_as_main_and_cannot_be_combined_with_bump_when() {
        assert_eq!(
            bump_when(&["--bump-only-if-same-as-main"]).unwrap(),
            BumpWhen::SameAsMain
        );

        let error = bump_when(&[
            "--bump-when",
            "not-bumped-on-branch",
            "--bump-only-if-same-as-main",
        ])
        .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::ArgumentConflict);
    }
}
