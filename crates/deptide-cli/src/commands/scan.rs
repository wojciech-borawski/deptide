use std::path::Path;

use deptide_core::error::{AppError, AppResult};
use deptide_core::scan::{
    apply_scan_selection, build_ignored_directories, detect_projects, DetectionOptions,
};
use deptide_core::workspace::{load_settings, open_with_config, save_config};

use crate::arguments::ScanArgs;
use crate::exit_code::ExitCode;

pub fn execute(args: ScanArgs) -> AppResult<ExitCode> {
    let (workspace, config) = open_with_config(&args.workspace)?;
    let settings = load_settings(&workspace);

    let root = Path::new(&settings.projects_root);
    if settings.projects_root.is_empty() || !root.is_dir() {
        return Err(AppError::new(
            "Set projectsRoot in settings.json before scanning",
        ));
    }

    let options = DetectionOptions {
        max_depth: settings.scan_depth,
        ignored_directories: build_ignored_directories(&settings.extra_ignored_directories),
    };
    let detected = detect_projects(root, &options);

    for project in &detected {
        let known = config
            .projects
            .iter()
            .any(|entry| workspace.resolve_project(&entry.path) == Path::new(&project.directory));
        println!(
            "{} {:<40} {}",
            if known { "*" } else { " " },
            project.proposed_name,
            project.relative_path
        );
    }
    println!(
        "{} projects found under {} (* already configured)",
        detected.len(),
        root.display()
    );

    if args.apply {
        let directories: Vec<String> = detected
            .iter()
            .map(|project| project.directory.clone())
            .collect();
        let outcome = apply_scan_selection(&workspace, &config, &detected, &directories);
        save_config(&workspace, &outcome.config)?;
        println!(
            "configuration updated: {} added, {} removed",
            outcome.added.len(),
            outcome.removed.len()
        );
    }

    Ok(ExitCode::Success)
}
