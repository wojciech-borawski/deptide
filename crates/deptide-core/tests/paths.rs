mod common;

use std::path::Path;

use common::TempDir;
use deptide_core::workspace::{lexical_normalize, normalize_directory, relative_path, Workspace};

#[test]
fn lexical_normalize_resolves_dot_segments_without_touching_disk() {
    let root = TempDir::new("paths");
    let messy = root.join("a/./b/../c");

    assert_eq!(lexical_normalize(&messy), root.join("a/c"));
}

#[test]
fn normalize_directory_is_case_insensitive_on_windows() {
    let root = TempDir::new("paths");
    let upper = root.join("Projects/Web");
    let lower = root.join("projects/web");

    if cfg!(windows) {
        assert_eq!(normalize_directory(&upper), normalize_directory(&lower));
    } else {
        assert_ne!(normalize_directory(&upper), normalize_directory(&lower));
    }
}

#[test]
fn relative_path_walks_up_and_down() {
    let root = TempDir::new("paths");
    let from = root.join("tool/config");
    let to = root.join("repos/web");

    let relative = relative_path(&from, &to).expect("same drive");
    assert_eq!(
        relative,
        Path::new("..").join("..").join("repos").join("web")
    );
}

#[test]
fn workspace_writes_posix_relative_paths_and_resolves_them_back() {
    let root = TempDir::new("paths");
    let workspace_dir = root.mkdir("tool");
    let project = root.mkdir("repos/web");

    let workspace = Workspace::open(&workspace_dir).expect("workspace opens");
    let stored = workspace.to_config_path(&project);

    assert_eq!(stored, "../repos/web");
    assert_eq!(
        workspace.resolve_project(&stored),
        lexical_normalize(&project)
    );
}

#[test]
fn workspace_prefers_legacy_config_location_when_only_that_exists() {
    let root = TempDir::new("paths");
    root.write("config/update-libs.json", "{}");

    let workspace = Workspace::open(root.path()).expect("workspace opens");
    assert_eq!(
        workspace.config_file(),
        root.join("config/update-libs.json")
    );
    assert_eq!(workspace.config_directory(), root.join("config"));

    root.write("update-libs.json", "{}");
    assert_eq!(workspace.config_file(), root.join("update-libs.json"));
}

#[test]
fn workspace_refuses_a_file_or_missing_folder() {
    let root = TempDir::new("paths");
    let file = root.write("not-a-folder.txt", "x");

    assert!(Workspace::open(&file).is_err());
    assert!(Workspace::open(root.join("missing")).is_err());
}
