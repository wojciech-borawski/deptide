mod common;

use common::{manifest, TempDir};
use deptide_core::domain::{ConfiguredProject, UpdateConfig};
use deptide_core::transfer::{
    analyze_receive, apply_receive, collect_files, preview, suggest_target, FileStatus,
    KnownProject, ReceiveRequest, ReceiveSelection,
};
use deptide_core::workspace::{save_config, Workspace};

fn fill_project(root: &TempDir, base: &str) {
    root.write(
        &format!("{base}/package.json"),
        &manifest("@acme/web", "1.0.0", &[]),
    );
    root.write(&format!("{base}/src/index.ts"), "export const a = 1;\n");
    root.write(&format!("{base}/src/generated/big.js"), "// generated\n");
    root.write(&format!("{base}/dist/bundle.js"), "bundle\n");
    root.write(&format!("{base}/node_modules/dep/index.js"), "dep\n");
    root.write(&format!("{base}/.env.local"), "SECRET=1\n");
    root.write(&format!("{base}/.gitignore"), "node_modules/\ndist/\n");
    root.write(&format!("{base}/.git/HEAD"), "ref: refs/heads/main\n");
}

#[test]
fn collecting_files_respects_gitignore_and_extra_patterns() {
    let root = TempDir::new("transfer-collect");
    fill_project(&root, "web");

    let plain = collect_files(&root.join("web"), &[], true);
    let names: Vec<String> = plain
        .files
        .iter()
        .map(|file| file.relative.to_string_lossy().replace('\\', "/"))
        .collect();

    assert_eq!(
        names,
        vec![
            ".env.local",
            ".gitignore",
            "package.json",
            "src/generated/big.js",
            "src/index.ts"
        ],
        "node_modules, dist and .git are left out"
    );
    assert_eq!(plain.skipped, 0);

    let filtered = collect_files(
        &root.join("web"),
        &["*.local".to_string(), "src/generated/".to_string()],
        true,
    );
    let filtered_names: Vec<String> = filtered
        .files
        .iter()
        .map(|file| file.relative.to_string_lossy().replace('\\', "/"))
        .collect();

    assert_eq!(
        filtered_names,
        vec![".gitignore", "package.json", "src/index.ts"]
    );
    assert_eq!(filtered.skipped, 2);

    let everything = collect_files(&root.join("web"), &[], false);
    assert!(everything
        .files
        .iter()
        .any(|file| file.relative.ends_with("bundle.js")));
}

#[test]
fn preview_combines_configured_and_run_patterns() {
    let root = TempDir::new("transfer-preview");
    fill_project(&root, "repos/web");
    let tool = root.mkdir("tool");

    let workspace = Workspace::open(&tool).unwrap();
    let config = UpdateConfig {
        projects: vec![ConfiguredProject::new("web", "../repos/web")],
        transfer_ignore: vec!["*.local".to_string()],
        ..UpdateConfig::default()
    };
    save_config(&workspace, &config).unwrap();

    let result = preview(
        &workspace,
        &config,
        &["web".to_string()],
        &["src/generated/".to_string()],
    )
    .unwrap();

    assert_eq!(result.projects.len(), 1);
    assert_eq!(result.projects[0].files, 3);
    assert_eq!(result.projects[0].skipped, 2);
    assert_eq!(result.patterns, vec!["*.local", "src/generated/"]);
    assert!(result.bytes > 0);

    assert!(preview(&workspace, &config, &["ghost".to_string()], &[]).is_err());
}

#[test]
fn clipboard_folders_are_matched_to_configured_projects() {
    let root = TempDir::new("transfer-match");
    root.write(
        "incoming/Storefront/package.json",
        &manifest("@acme/storefront", "1.0.0", &[]),
    );
    root.write(
        "incoming/mystery/package.json",
        &manifest("@acme/api", "1.0.0", &[]),
    );
    root.write(
        "incoming/unknown/package.json",
        &manifest("@acme/nothing", "1.0.0", &[]),
    );

    let known = vec![
        KnownProject {
            name: "storefront".to_string(),
            directory: root.join("repos/dash"),
            package_name: Some("@acme/storefront".to_string()),
        },
        KnownProject {
            name: "api".to_string(),
            directory: root.join("repos/api"),
            package_name: Some("@acme/api".to_string()),
        },
    ];

    assert_eq!(
        suggest_target(&root.join("incoming/Storefront"), &known).as_deref(),
        Some("storefront")
    );
    assert_eq!(
        suggest_target(&root.join("incoming/mystery"), &known).as_deref(),
        Some("api")
    );
    assert!(suggest_target(&root.join("incoming/unknown"), &known).is_none());
}

#[test]
fn receiving_classifies_files_and_copies_only_the_selection() {
    let root = TempDir::new("transfer-receive");
    root.write(
        "incoming/web/package.json",
        &manifest("@acme/web", "1.1.0", &[]),
    );
    root.write("incoming/web/src/index.ts", "export const a = 2;\n");
    root.write("incoming/web/src/new.ts", "export const b = 1;\n");
    root.write("incoming/web/README.md", "same\n");
    root.write("incoming/web/secret.local", "no\n");

    root.write(
        "repos/web/package.json",
        &manifest("@acme/web", "1.0.0", &[]),
    );
    root.write("repos/web/src/index.ts", "export const a = 1;\n");
    root.write("repos/web/README.md", "same\n");
    let tool = root.mkdir("tool");

    let workspace = Workspace::open(&tool).unwrap();
    let config = UpdateConfig {
        projects: vec![ConfiguredProject::new("web", "../repos/web")],
        ..UpdateConfig::default()
    };

    let plan = analyze_receive(
        &workspace,
        &config,
        &[ReceiveRequest {
            source: root.join("incoming/web").to_string_lossy().to_string(),
            target: "web".to_string(),
        }],
        &["*.local".to_string()],
    )
    .unwrap();

    let project = &plan.projects[0];
    assert_eq!(
        (
            project.added,
            project.replaced,
            project.identical,
            project.skipped
        ),
        (1, 2, 1, 1)
    );
    let status = |name: &str| {
        project
            .files
            .iter()
            .find(|file| file.relative == name)
            .unwrap()
            .status
    };
    assert_eq!(status("src/new.ts"), FileStatus::Added);
    assert_eq!(status("src/index.ts"), FileStatus::Replaced);
    assert_eq!(status("README.md"), FileStatus::Identical);

    let result = apply_receive(
        &workspace,
        &config,
        &[ReceiveSelection {
            source: project.source.clone(),
            target: "web".to_string(),
            files: vec![
                "src/new.ts".to_string(),
                "src/index.ts".to_string(),
                "../escape.txt".to_string(),
            ],
        }],
    )
    .unwrap();

    assert_eq!(result.projects[0].added, 1);
    assert_eq!(result.projects[0].replaced, 1);
    assert_eq!(result.files, 2);
    assert!(std::fs::read_to_string(root.join("repos/web/src/index.ts"))
        .unwrap()
        .contains("= 2"));
    assert!(root.join("repos/web/src/new.ts").exists());
    assert!(!root.join("repos/escape.txt").exists());
    assert!(
        std::fs::read_to_string(root.join("repos/web/package.json"))
            .unwrap()
            .contains("1.0.0"),
        "unselected files are untouched"
    );
    assert!(result.log_file.is_some());
}
