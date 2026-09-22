use clap::{Args, Parser, Subcommand, ValueEnum};

use deptide_core::domain::{ExecutionMode, StepName, VersionBump};

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
        help = "Let the version step bump only projects whose version still equals the one on the main branch"
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
