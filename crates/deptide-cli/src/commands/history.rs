use deptide_core::error::AppResult;
use deptide_core::execution::list_summaries;
use deptide_core::util::time::format_duration;
use deptide_core::workspace::Workspace;

use crate::arguments::HistoryArgs;
use crate::exit_code::ExitCode;

pub fn execute(args: HistoryArgs) -> AppResult<ExitCode> {
    let workspace = Workspace::open(&args.workspace)?;
    let summaries = list_summaries(&workspace.logs_directory());

    if summaries.is_empty() {
        println!("no runs recorded yet");
        return Ok(ExitCode::Success);
    }

    for summary in summaries.iter().take(args.limit) {
        let outcome = if summary.aborted {
            "stopped".to_string()
        } else if summary.failed_count > 0 {
            format!("{} failed", summary.failed_count)
        } else if summary.warn_count > 0 {
            format!("{} warnings", summary.warn_count)
        } else {
            "ok".to_string()
        };
        println!(
            "{}  {:<32} {:>3} projects  {:>9}  {}",
            &summary.started_at[..16.min(summary.started_at.len())],
            summary.label,
            summary.project_count,
            format_duration(summary.total_duration_ms),
            outcome
        );
    }

    Ok(ExitCode::Success)
}
