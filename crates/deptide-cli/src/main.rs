mod arguments;
mod commands;
mod exit_code;
mod sink;

use clap::Parser;

use arguments::{Cli, Command};
use exit_code::ExitCode;

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    deptide_core::execution::install_crash_hook();
    let cli = Cli::parse();

    let outcome = match cli.command {
        Command::Scan(args) => commands::scan::execute(args),
        Command::Run(args) => commands::run::execute(args).await,
        Command::Exec(args) => commands::exec::execute(args).await,
        Command::History(args) => commands::history::execute(args),
        Command::Deps(args) => commands::deps::execute(args).await,
        Command::Groups(args) => commands::groups::execute(args),
        Command::Restore(args) => commands::restore::execute(args),
    };

    let code = match outcome {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::Usage
        }
    };

    std::process::exit(code as i32);
}
