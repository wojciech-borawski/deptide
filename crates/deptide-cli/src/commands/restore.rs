use deptide_core::domain::{ProjectRestore, RestoreOutcome};
use deptide_core::error::AppResult;
use deptide_core::execution::{restore_run, RestoreRequest};
use deptide_core::workspace::Workspace;

use crate::arguments::RestoreArgs;
use crate::exit_code::ExitCode;

fn restore_lines(results: &[ProjectRestore]) -> Vec<String> {
    let mut lines: Vec<String> = results
        .iter()
        .map(|result| match &result.outcome {
            RestoreOutcome::Restored { files } => {
                format!("restored {}: {}", result.name, files.join(", "))
            }
            RestoreOutcome::Failed { error } => format!("failed   {}: {error}", result.name),
        })
        .collect();

    if results.iter().any(is_restored) {
        lines.push(
            "run npm install in the restored projects so node_modules matches the files again"
                .to_string(),
        );
    }

    lines
}

fn is_restored(result: &ProjectRestore) -> bool {
    matches!(result.outcome, RestoreOutcome::Restored { .. })
}

pub fn execute(args: RestoreArgs) -> AppResult<ExitCode> {
    let workspace = Workspace::open(&args.workspace)?;
    let results = restore_run(
        &workspace,
        &RestoreRequest {
            run: args.run,
            projects: args.projects,
        },
    )?;

    for line in restore_lines(&results) {
        println!("{line}");
    }

    Ok(if results.iter().all(is_restored) {
        ExitCode::Success
    } else {
        ExitCode::Failures
    })
}

#[cfg(test)]
mod tests {
    use deptide_core::domain::{ProjectRestore, RestoreOutcome};

    use super::restore_lines;

    #[test]
    fn each_project_reports_its_files_or_its_error_and_npm_install_is_suggested() {
        let results = vec![
            ProjectRestore {
                name: "web".to_string(),
                outcome: RestoreOutcome::Restored {
                    files: vec!["package.json".to_string(), "package-lock.json".to_string()],
                },
            },
            ProjectRestore {
                name: "api".to_string(),
                outcome: RestoreOutcome::Failed {
                    error: "access denied".to_string(),
                },
            },
        ];

        assert_eq!(
            restore_lines(&results),
            vec![
                "restored web: package.json, package-lock.json",
                "failed   api: access denied",
                "run npm install in the restored projects so node_modules matches the files again",
            ]
        );
    }
}
