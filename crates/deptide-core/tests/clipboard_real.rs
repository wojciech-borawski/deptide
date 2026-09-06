mod common;

use common::{manifest, TempDir};
use deptide_core::domain::{ConfiguredProject, UpdateConfig};
use deptide_core::transfer::{copy_to_clipboard, inspect_clipboard};
use deptide_core::workspace::{save_config, Workspace};

#[test]
#[ignore = "touches the real system clipboard"]
fn staged_folders_round_trip_through_the_clipboard() {
    let root = TempDir::new("clipboard");
    root.write(
        "repos/web/package.json",
        &manifest("@acme/web", "1.0.0", &[]),
    );
    root.write("repos/web/src/index.ts", "export const a = 1;\n");
    root.write("repos/web/node_modules/dep/index.js", "dep\n");
    root.write("repos/web/.gitignore", "node_modules/\n");
    let tool = root.mkdir("tool");

    let workspace = Workspace::open(&tool).unwrap();
    let config = UpdateConfig {
        projects: vec![ConfiguredProject::new("web", "../repos/web")],
        ..UpdateConfig::default()
    };
    save_config(&workspace, &config).unwrap();

    let result = copy_to_clipboard(&workspace, &config, &["web".to_string()], &[]).unwrap();
    assert_eq!(result.projects[0].files, 3);
    let staged = result.projects[0].staged_path.clone().unwrap();
    assert!(std::path::Path::new(&staged).join("src/index.ts").exists());
    assert!(!std::path::Path::new(&staged).join("node_modules").exists());

    let contents = inspect_clipboard(&workspace, &config);
    let entry = contents
        .entries
        .iter()
        .find(|entry| entry.path.eq_ignore_ascii_case(&staged))
        .expect("the staged folder is on the clipboard");
    assert!(entry.is_project);
    assert_eq!(entry.package_name.as_deref(), Some("@acme/web"));
    assert_eq!(entry.suggested_project.as_deref(), Some("web"));
}
