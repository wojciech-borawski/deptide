mod candidates;
mod configure;
mod detect;
mod duplicates;
mod git;

pub use candidates::{apply_suffix, collect_dependency_candidates, strip_prerelease};
pub use configure::{apply_scan_selection, ScanSelectionOutcome};
pub use detect::{
    build_ignored_directories, detect_projects, propose_name, read_package_manifest,
    read_project_at, DetectionOptions, BUILD_SCRIPT_NAME, DEFAULT_IGNORED_DIRECTORIES,
    MANIFEST_FILE_NAME,
};
pub use duplicates::{find_duplicate_groups, DuplicateGroup};
pub use git::{
    branch_suffix, compile_branch_pattern, current_branch, version_on_main_branch,
    DEFAULT_BRANCH_SUFFIX_PATTERN,
};
