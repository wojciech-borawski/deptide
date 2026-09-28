mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use common::{manifest, TempDir};
use deptide_core::domain::{ConfiguredProject, UpdateConfig};
use deptide_core::error::{AppError, AppResult};
use deptide_core::transfer::{
    analyze_receive, apply_receive, apply_receive_with, collect_files, preview, suggest_target,
    suggest_target_for, FileStatus, KnownProject, ReceiveProjectPlan, ReceiveRequest,
    ReceiveSelection, Recycler,
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

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

/// Moves recycled files into `bin` under their path relative to `base`, and
/// records every batch as posix paths relative to `base`.
struct BinRecycler {
    base: PathBuf,
    bin: PathBuf,
    fail_on_batch: Option<usize>,
    batches: Mutex<Vec<Vec<String>>>,
}

impl BinRecycler {
    fn new(base: &Path, fail_on_batch: Option<usize>) -> Self {
        Self {
            base: base.to_path_buf(),
            bin: base.join("bin"),
            fail_on_batch,
            batches: Mutex::new(Vec::new()),
        }
    }

    fn batches(&self) -> Vec<Vec<String>> {
        self.batches.lock().unwrap().clone()
    }
}

impl Recycler for BinRecycler {
    fn recycle(&self, paths: &[PathBuf]) -> AppResult<()> {
        let relative: Vec<String> = paths
            .iter()
            .map(|path| {
                path.strip_prefix(&self.base)
                    .expect("recycled paths stay inside the test folder")
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();

        let mut batches = self.batches.lock().unwrap();
        batches.push(relative.clone());
        if self.fail_on_batch == Some(batches.len() - 1) {
            return Err(AppError::new("the bin is full"));
        }

        for (path, relative) in paths.iter().zip(&relative) {
            let parked = self.bin.join(relative);
            fs::create_dir_all(parked.parent().unwrap())?;
            fs::rename(path, parked)?;
        }
        Ok(())
    }
}

/// A workspace with one project `web` in `repos/web` and a received copy of
/// it in `incoming/web`.
struct ReceiveSetup {
    root: TempDir,
    workspace: Workspace,
    config: UpdateConfig,
}

impl ReceiveSetup {
    fn new(label: &str, transfer_ignore: &[&str]) -> Self {
        let root = TempDir::new(label);
        root.mkdir("incoming/web");
        root.mkdir("repos/web");
        let tool = root.mkdir("tool");
        let workspace = Workspace::open(&tool).unwrap();
        let config = UpdateConfig {
            projects: vec![ConfiguredProject::new("web", "../repos/web")],
            transfer_ignore: strings(transfer_ignore),
            ..UpdateConfig::default()
        };

        Self {
            root,
            workspace,
            config,
        }
    }

    fn incoming(&self, relative: &str, content: &str) {
        self.root
            .write(&format!("incoming/web/{relative}"), content);
    }

    fn target(&self, relative: &str, content: &str) {
        self.root.write(&format!("repos/web/{relative}"), content);
    }

    fn target_path(&self, relative: &str) -> PathBuf {
        self.root.join("repos/web").join(relative)
    }

    fn read_target(&self, relative: &str) -> String {
        fs::read_to_string(self.target_path(relative)).unwrap()
    }

    fn source(&self) -> String {
        self.root.join("incoming/web").to_string_lossy().to_string()
    }

    fn plan(&self, extra: &[&str], disabled: &[&str]) -> ReceiveProjectPlan {
        analyze_receive(
            &self.workspace,
            &self.config,
            &[ReceiveRequest {
                source: self.source(),
                target: "web".to_string(),
            }],
            &strings(extra),
            &strings(disabled),
        )
        .unwrap()
        .projects
        .remove(0)
    }

    fn selection(&self, files: &[&str], delete: &[&str]) -> ReceiveSelection {
        ReceiveSelection {
            source: self.source(),
            target: "web".to_string(),
            files: strings(files),
            delete: strings(delete),
        }
    }

    fn recycler(&self, fail_on_batch: Option<usize>) -> BinRecycler {
        BinRecycler::new(self.root.path(), fail_on_batch)
    }
}

fn statuses(plan: &ReceiveProjectPlan) -> Vec<(&str, FileStatus)> {
    plan.files
        .iter()
        .map(|file| (file.relative.as_str(), file.status))
        .collect()
}

fn counts(plan: &ReceiveProjectPlan) -> (usize, usize, usize, usize, usize, usize) {
    (
        plan.added,
        plan.replaced,
        plan.whitespace,
        plan.identical,
        plan.removed,
        plan.skipped,
    )
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
        &[],
    )
    .unwrap();

    assert_eq!(result.projects.len(), 1);
    assert_eq!(result.projects[0].files, 3);
    assert_eq!(result.projects[0].skipped, 2);
    assert_eq!(result.patterns, vec!["*.local", "src/generated/"]);
    assert!(result.bytes > 0);

    assert!(preview(&workspace, &config, &["ghost".to_string()], &[], &[]).is_err());
}

#[test]
fn a_configured_pattern_disabled_for_one_preview_lets_its_files_through() {
    let root = TempDir::new("transfer-preview-disabled");
    fill_project(&root, "repos/web");
    let tool = root.mkdir("tool");

    let workspace = Workspace::open(&tool).unwrap();
    let config = UpdateConfig {
        projects: vec![ConfiguredProject::new("web", "../repos/web")],
        transfer_ignore: strings(&["*.local", "src/generated/"]),
        ..UpdateConfig::default()
    };
    let web = ["web".to_string()];

    let result = preview(&workspace, &config, &web, &[], &strings(&[" *.local "])).unwrap();
    assert_eq!(result.patterns, vec!["src/generated/"]);
    assert_eq!(
        (result.projects[0].files, result.projects[0].skipped),
        (4, 1),
        ".env.local is counted again"
    );

    let result = preview(
        &workspace,
        &config,
        &web,
        &strings(&["*.local"]),
        &strings(&["*.local"]),
    )
    .unwrap();
    assert_eq!(
        result.patterns,
        vec!["src/generated/", "*.local"],
        "a run pattern is not turned off by a disabled configured one"
    );
    assert_eq!(result.projects[0].files, 3);
    assert_eq!(
        config.transfer_ignore,
        strings(&["*.local", "src/generated/"])
    );
}

#[test]
fn a_configured_pattern_disabled_for_one_receive_lets_its_files_through() {
    let setup = ReceiveSetup::new("transfer-receive-disabled", &["*.local"]);
    setup.incoming("package.json", "{}\n");
    setup.incoming("secret.local", "incoming\n");
    setup.target("package.json", "{}\n");
    setup.target("notes.local", "local\n");

    let plan = setup.plan(&[], &[]);
    assert_eq!(
        statuses(&plan),
        vec![("package.json", FileStatus::Identical)]
    );
    assert_eq!(plan.skipped, 1);

    let plan = setup.plan(&[], &["*.local"]);
    assert_eq!(
        statuses(&plan),
        vec![
            ("notes.local", FileStatus::Removed),
            ("package.json", FileStatus::Identical),
            ("secret.local", FileStatus::Added),
        ]
    );
    assert_eq!(plan.skipped, 0);
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
fn a_virtual_folder_is_matched_by_its_name_or_package_name() {
    let known = vec![
        KnownProject {
            name: "storefront".to_string(),
            directory: PathBuf::from("C:\\repos\\dash"),
            package_name: Some("@acme/storefront".to_string()),
        },
        KnownProject {
            name: "api".to_string(),
            directory: PathBuf::from("C:\\repos\\orders-api"),
            package_name: Some("@acme/api".to_string()),
        },
    ];

    assert_eq!(
        suggest_target_for("Storefront", None, &known).as_deref(),
        Some("storefront")
    );
    assert_eq!(
        suggest_target_for("ORDERS-API", None, &known).as_deref(),
        Some("api")
    );
    assert_eq!(
        suggest_target_for("copy of dash", Some("@acme/storefront"), &known).as_deref(),
        Some("storefront")
    );
    assert_eq!(
        suggest_target_for("api", Some("@acme/storefront"), &known).as_deref(),
        Some("api"),
        "the folder name wins over the package name"
    );
    assert!(suggest_target_for("unknown", Some("@acme/nothing"), &known).is_none());
}

#[test]
fn a_gitignore_above_a_received_folder_does_not_hide_its_files() {
    let setup = ReceiveSetup::new("transfer-parent-ignore", &[]);
    setup.root.write(".gitignore", "*.generated.ts\nvendor/\n");
    setup.incoming("package.json", "{}\n");
    setup.incoming("src/api.generated.ts", "export const api = 1;\n");
    setup.incoming("vendor/lib.js", "lib\n");
    setup.incoming(".gitignore", "secret.txt\n");
    setup.incoming("secret.txt", "no\n");
    setup.target("package.json", "{}\n");
    setup.target("src/old.generated.ts", "export const old = 1;\n");

    let plan = setup.plan(&[], &[]);

    assert_eq!(
        statuses(&plan),
        vec![
            (".gitignore", FileStatus::Added),
            ("package.json", FileStatus::Identical),
            ("src/api.generated.ts", FileStatus::Added),
            ("vendor/lib.js", FileStatus::Added),
        ],
        "the received folder's own .gitignore applies, the one above it does not, and the target keeps both"
    );

    let copied: Vec<String> = collect_files(&setup.root.join("incoming/web"), &[], true)
        .files
        .iter()
        .map(|file| file.relative.to_string_lossy().replace('\\', "/"))
        .collect();
    assert_eq!(
        copied,
        vec![".gitignore", "package.json"],
        "Copy still reads ignore files above the project"
    );
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
    root.write("repos/web/src/old.ts", "export const gone = 1;\n");
    root.write("repos/web/notes.local", "no\n");
    root.write("repos/web/node_modules/dep/index.js", "dep\n");
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
        &[],
    )
    .unwrap();

    let project = &plan.projects[0];
    assert_eq!(counts(project), (1, 2, 0, 1, 1, 1));
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
    assert_eq!(status("src/old.ts"), FileStatus::Removed);

    let recycler = BinRecycler::new(root.path(), None);
    let result = apply_receive_with(
        &workspace,
        &config,
        &[ReceiveSelection {
            source: project.source.clone(),
            target: "web".to_string(),
            files: strings(&["src/new.ts", "src/index.ts", "../escape.txt"]),
            delete: Vec::new(),
        }],
        &recycler,
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
    assert!(root.join("repos/web/src/old.ts").exists());
    assert!(result.log_file.is_some());
}

#[test]
fn line_ending_trailing_space_and_blank_line_changes_are_whitespace_only() {
    let setup = ReceiveSetup::new("transfer-whitespace", &[]);
    let cases = [
        (
            "binary.bin",
            "a\0\r\nb\r\n",
            "a\0\nb\n",
            FileStatus::Replaced,
        ),
        ("blank.ts", "a\n\n  \n\nb", "a\nb\n", FileStatus::Whitespace),
        ("cr.ts", "a\rb\r", "a\nb\n", FileStatus::Whitespace),
        ("crlf.ts", "a\r\nb\r\n", "a\nb\n", FileStatus::Whitespace),
        ("edit.ts", "a\nb\n", "a\nc\n", FileStatus::Replaced),
        ("indent.ts", "  a\n", "a\n", FileStatus::Replaced),
        ("inner.ts", "a  b\n", "a b\n", FileStatus::Replaced),
        ("same.ts", "a\n", "a\n", FileStatus::Identical),
        ("tabs.ts", "\ta\n", "    a\n", FileStatus::Replaced),
        (
            "trailing.ts",
            "a  \nb\t\n",
            "a\nb\n",
            FileStatus::Whitespace,
        ),
    ];
    for (name, incoming, target, _) in &cases {
        setup.incoming(name, incoming);
        setup.target(name, target);
    }

    let plan = setup.plan(&[], &[]);

    let expected: Vec<(&str, FileStatus)> = cases
        .iter()
        .map(|(name, _, _, status)| (*name, *status))
        .collect();
    assert_eq!(statuses(&plan), expected);
    assert_eq!(counts(&plan), (0, 5, 4, 1, 0, 0));
}

#[test]
fn large_files_are_never_whitespace_only_and_binary_is_judged_by_the_first_8_kib() {
    let setup = ReceiveSetup::new("transfer-whitespace-large", &[]);
    let line = "0123456789".repeat(6);
    let lf = |lines: usize| format!("{line}\n").repeat(lines);
    let crlf = |lines: usize| format!("{line}\r\n").repeat(lines);
    let limit = 5 * 1024 * 1024;
    assert!(crlf(80_000).len() < limit && lf(87_000).len() > limit);
    assert!(crlf(85_000).len() > limit && lf(85_000).len() < limit);

    setup.incoming("both-under.txt", &crlf(80_000));
    setup.target("both-under.txt", &lf(80_000));
    setup.incoming("both-over.txt", &crlf(87_000));
    setup.target("both-over.txt", &lf(87_000));
    setup.incoming("incoming-over.txt", &crlf(85_000));
    setup.target("incoming-over.txt", &lf(85_000));
    setup.incoming("target-over.txt", &lf(85_000));
    setup.target("target-over.txt", &crlf(85_000));

    let text = "x".repeat(9 * 1024);
    setup.incoming("late-nul.txt", &format!("{text}\r\n\0\r\n"));
    setup.target("late-nul.txt", &format!("{text}\n\0\n"));

    let plan = setup.plan(&[], &[]);

    assert_eq!(
        statuses(&plan),
        vec![
            ("both-over.txt", FileStatus::Replaced),
            ("both-under.txt", FileStatus::Whitespace),
            ("incoming-over.txt", FileStatus::Replaced),
            ("late-nul.txt", FileStatus::Whitespace),
            ("target-over.txt", FileStatus::Replaced),
        ]
    );
}

#[test]
fn files_only_in_the_target_are_removed_unless_ignored() {
    let setup = ReceiveSetup::new("transfer-removed", &["*.local"]);
    // Re-includes node_modules over any .gitignore above the temp folder.
    let whitelist = "!node_modules/\n";
    setup.incoming(".gitignore", whitelist);
    setup.incoming("package.json", "{}\n");
    setup.incoming("src/index.ts", "export const a = 2;\n");
    setup.incoming("src/new.ts", "export const b = 1;\n");
    setup.incoming("secret.local", "incoming\n");

    setup.target(".gitignore", whitelist);
    setup.target("package.json", "{}\n");
    setup.target("src/index.ts", "export const a = 1;\n");
    setup.target("src/legacy.ts", "legacy\n");
    setup.target("src/old.ts", "export const gone = 1;\n");
    setup.target("notes.local", "local\n");
    setup.target("node_modules/dep/index.js", "dep\n");
    setup.target(".git/HEAD", "ref: refs/heads/main\n");

    let plan = setup.plan(&[], &[]);

    assert_eq!(
        statuses(&plan),
        vec![
            (".gitignore", FileStatus::Identical),
            ("package.json", FileStatus::Identical),
            ("src/index.ts", FileStatus::Replaced),
            ("src/legacy.ts", FileStatus::Removed),
            ("src/new.ts", FileStatus::Added),
            ("src/old.ts", FileStatus::Removed),
        ],
        "ignored, node_modules and .git files in the target are not removal candidates"
    );
    assert_eq!(counts(&plan), (1, 1, 0, 2, 2, 1));
    let old = plan
        .files
        .iter()
        .find(|file| file.relative == "src/old.ts")
        .unwrap();
    assert_eq!(old.size, "export const gone = 1;\n".len() as u64);
}

#[cfg(windows)]
#[test]
fn a_target_file_whose_name_differs_only_in_case_is_not_removed() {
    let setup = ReceiveSetup::new("transfer-removed-case", &[]);
    setup.incoming("src/App.vue", "new\n");
    setup.target("src/app.vue", "old\n");

    let plan = setup.plan(&[], &[]);

    assert_eq!(statuses(&plan), vec![("src/App.vue", FileStatus::Replaced)]);
}

#[test]
fn applying_recycles_overwritten_and_deleted_files_before_copying() {
    let setup = ReceiveSetup::new("transfer-apply", &[]);
    setup.incoming("src/index.ts", "export const a = 2;\n");
    setup.incoming("src/new.ts", "export const b = 1;\n");
    setup.incoming("present.ts", "incoming\n");
    setup.incoming("docs", "a file where the target has a folder\n");
    setup.incoming("lib/x.ts", "under a folder the target has as a file\n");

    setup.target("src/index.ts", "export const a = 1;\n");
    setup.target("src/old/gone.ts", "gone\n");
    setup.target("src/old/deeper/also.ts", "also\n");
    setup.target("kept.ts", "kept\n");
    setup.target("present.ts", "local\n");
    setup.target("docs/readme.md", "docs\n");
    setup.target("lib", "a file\n");
    setup.target(".git/config", "[core]\n");
    setup.root.mkdir("repos/web/empty");
    setup.root.write("repos/outside.txt", "outside\n");
    setup.root.write("incoming/escape.txt", "escape\n");
    let absolute_file = setup.root.write("elsewhere/absolute.txt", "absolute\n");
    let absolute = absolute_file.to_string_lossy().to_string();

    let recycler = setup.recycler(None);
    let result = apply_receive_with(
        &setup.workspace,
        &setup.config,
        &[setup.selection(
            &[
                "src/index.ts",
                "src/new.ts",
                "src/new.ts",
                "ghost.ts",
                "../escape.txt",
                &absolute,
                "",
                "/src/new.ts",
                "docs",
                "lib/x.ts",
            ],
            &[
                "src/old/gone.ts",
                "src/old/deeper/also.ts",
                "src/old/gone.ts",
                "present.ts",
                "missing.ts",
                "../outside.txt",
                &absolute,
                ".git/config",
                "empty",
            ],
        )],
        &recycler,
    )
    .unwrap();

    let mut batches = recycler.batches();
    assert_eq!(batches.len(), 1, "one batch per project");
    batches[0].sort();
    assert_eq!(
        batches[0],
        vec![
            "repos/web/src/index.ts",
            "repos/web/src/old/deeper/also.ts",
            "repos/web/src/old/gone.ts",
        ]
    );
    assert_eq!(
        fs::read_to_string(setup.root.join("bin/repos/web/src/index.ts")).unwrap(),
        "export const a = 1;\n",
        "the bin holds the version from before the copy"
    );

    assert_eq!(setup.read_target("src/index.ts"), "export const a = 2;\n");
    assert!(setup.target_path("src/new.ts").is_file());
    assert!(
        !setup.target_path("src/old").exists(),
        "folders emptied by the deletions are removed"
    );
    assert!(setup.target_path("src").is_dir());
    assert!(
        setup.target_path("empty").is_dir(),
        "an empty folder the deletions did not empty stays"
    );
    assert_eq!(setup.read_target("kept.ts"), "kept\n");
    assert_eq!(setup.read_target("present.ts"), "local\n");
    assert_eq!(setup.read_target("docs/readme.md"), "docs\n");
    assert_eq!(setup.read_target("lib"), "a file\n");
    assert_eq!(setup.read_target(".git/config"), "[core]\n");
    assert!(setup.root.join("repos/outside.txt").is_file());
    assert!(!setup.root.join("repos/escape.txt").exists());
    assert_eq!(fs::read_to_string(&absolute_file).unwrap(), "absolute\n");

    let project = &result.projects[0];
    assert_eq!(
        (
            project.added,
            project.replaced,
            project.deleted,
            project.recycled
        ),
        (1, 1, 2, 3)
    );
    assert_eq!(project.files, vec!["src/index.ts", "src/new.ts"]);
    assert_eq!(
        project.deleted_files,
        vec!["src/old/gone.ts", "src/old/deeper/also.ts"]
    );
    assert_eq!(
        project.skipped,
        vec![
            "ghost.ts",
            "../escape.txt",
            absolute.as_str(),
            "",
            "/src/new.ts",
            "docs",
            "lib/x.ts",
            "present.ts",
            "missing.ts",
            "../outside.txt",
            absolute.as_str(),
            ".git/config",
            "empty",
        ]
    );

    let journal: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(result.log_file.unwrap()).unwrap()).unwrap();
    assert_eq!(
        journal["projects"][0]["deletedFiles"],
        serde_json::json!(["src/old/gone.ts", "src/old/deeper/also.ts"])
    );
}

/// Makes `link` a folder link to `target`: a junction on Windows, which needs
/// no extra rights, and a symlink elsewhere.
fn link_folder(link: &Path, target: &Path) {
    #[cfg(windows)]
    {
        let native = |path: &Path| path.components().collect::<PathBuf>();
        let status = std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(native(link))
            .arg(native(target))
            .stdout(std::process::Stdio::null())
            .status()
            .expect("cmd can be started");
        assert!(status.success(), "mklink /J failed");
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).expect("symlink can be created");
}

#[test]
fn deletions_never_go_through_a_link_in_the_target() {
    let setup = ReceiveSetup::new("transfer-apply-links", &[]);
    setup.root.write("elsewhere/shared.ts", "shared\n");
    setup.target("real/local.ts", "local\n");
    link_folder(&setup.target_path("outward"), &setup.root.join("elsewhere"));
    link_folder(&setup.target_path("inward"), &setup.target_path("real"));
    assert_eq!(setup.read_target("outward/shared.ts"), "shared\n");

    let recycler = setup.recycler(None);
    let result = apply_receive_with(
        &setup.workspace,
        &setup.config,
        &[setup.selection(&[], &["outward/shared.ts", "inward/local.ts"])],
        &recycler,
    )
    .unwrap();

    assert!(recycler.batches().is_empty());
    assert_eq!(
        result.projects[0].skipped,
        vec!["outward/shared.ts", "inward/local.ts"]
    );
    assert_eq!(
        fs::read_to_string(setup.root.join("elsewhere/shared.ts")).unwrap(),
        "shared\n"
    );
    assert_eq!(setup.read_target("real/local.ts"), "local\n");
    assert!(setup.target_path("inward").exists());
}

#[cfg(windows)]
#[test]
fn windows_name_aliases_are_skipped() {
    let setup = ReceiveSetup::new("transfer-apply-aliases", &[]);
    setup.incoming("src/new.ts", "new\n");
    setup.incoming("src/new.ts:extra", "stream\n");
    setup.target("kept.ts", "kept\n");

    let recycler = setup.recycler(None);
    let result = apply_receive_with(
        &setup.workspace,
        &setup.config,
        &[setup.selection(
            &["src/new.ts.", "src/new.ts:extra", "src/new.ts "],
            &["kept.ts.", "KEPT.ts", "kept.ts::$DATA"],
        )],
        &recycler,
    )
    .unwrap();

    let project = &result.projects[0];
    assert!(project.files.is_empty(), "{:?}", project.files);
    assert!(
        project.deleted_files.is_empty(),
        "{:?}",
        project.deleted_files
    );
    assert!(recycler.batches().is_empty());
    assert!(!setup.target_path("src").exists());
    assert_eq!(setup.read_target("kept.ts"), "kept\n");
}

#[test]
fn deleting_every_file_keeps_the_target_folder() {
    let setup = ReceiveSetup::new("transfer-apply-root", &[]);
    setup.target("lonely/one.txt", "one\n");

    let recycler = setup.recycler(None);
    let result = apply_receive_with(
        &setup.workspace,
        &setup.config,
        &[setup.selection(&[], &["lonely/one.txt"])],
        &recycler,
    )
    .unwrap();

    assert_eq!(result.projects[0].deleted, 1);
    assert!(!setup.target_path("lonely").exists());
    assert!(setup.root.join("repos/web").is_dir());
}

#[test]
fn a_failed_recycle_stops_the_project_before_anything_is_copied() {
    let setup = ReceiveSetup::new("transfer-apply-fail", &[]);
    setup.incoming("src/index.ts", "export const a = 2;\n");
    setup.incoming("src/new.ts", "export const b = 1;\n");
    setup.target("src/index.ts", "export const a = 1;\n");
    setup.target("src/old.ts", "old\n");

    let recycler = setup.recycler(Some(0));
    let error = apply_receive_with(
        &setup.workspace,
        &setup.config,
        &[setup.selection(&["src/index.ts", "src/new.ts"], &["src/old.ts"])],
        &recycler,
    )
    .unwrap_err();

    assert!(error.message().contains("web"), "{error}");
    assert!(error.message().contains("the bin is full"), "{error}");
    assert_eq!(recycler.batches().len(), 1);
    assert_eq!(setup.read_target("src/index.ts"), "export const a = 1;\n");
    assert!(!setup.target_path("src/new.ts").exists());
    assert!(setup.target_path("src/old.ts").is_file());
    assert!(
        !setup.workspace.transfers_directory().exists(),
        "no journal when nothing was received"
    );
}

#[test]
fn an_unknown_target_stops_the_receive_before_any_file_moves() {
    let setup = ReceiveSetup::new("transfer-apply-unknown", &[]);
    setup.incoming("a.ts", "new\n");
    setup.target("a.ts", "old\n");
    let ghost = ReceiveSelection {
        target: "ghost".to_string(),
        ..setup.selection(&["a.ts"], &[])
    };

    let recycler = setup.recycler(None);
    let error = apply_receive_with(
        &setup.workspace,
        &setup.config,
        &[setup.selection(&["a.ts"], &[]), ghost],
        &recycler,
    )
    .unwrap_err();

    assert!(error.message().contains("ghost"), "{error}");
    assert!(recycler.batches().is_empty());
    assert_eq!(setup.read_target("a.ts"), "old\n");
}

#[test]
fn a_failed_project_leaves_a_journal_of_the_projects_received_before_it() {
    let root = TempDir::new("transfer-apply-partial");
    root.write("incoming/web/a.ts", "new a\n");
    root.write("repos/web/a.ts", "old a\n");
    root.write("incoming/api/b.ts", "new b\n");
    root.write("repos/api/b.ts", "old b\n");
    let tool = root.mkdir("tool");
    let workspace = Workspace::open(&tool).unwrap();
    let config = UpdateConfig {
        projects: vec![
            ConfiguredProject::new("web", "../repos/web"),
            ConfiguredProject::new("api", "../repos/api"),
        ],
        ..UpdateConfig::default()
    };
    let selection = |name: &str, file: &str| ReceiveSelection {
        source: root
            .join(&format!("incoming/{name}"))
            .to_string_lossy()
            .to_string(),
        target: name.to_string(),
        files: strings(&[file]),
        delete: Vec::new(),
    };

    let recycler = BinRecycler::new(root.path(), Some(1));
    let error = apply_receive_with(
        &workspace,
        &config,
        &[selection("web", "a.ts"), selection("api", "b.ts")],
        &recycler,
    )
    .unwrap_err();

    assert!(error.message().contains("api"), "{error}");
    assert!(error.message().contains("web"), "{error}");
    assert_eq!(
        fs::read_to_string(root.join("repos/web/a.ts")).unwrap(),
        "new a\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("repos/api/b.ts")).unwrap(),
        "old b\n"
    );

    let journals: Vec<PathBuf> = fs::read_dir(workspace.transfers_directory())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(journals.len(), 1);
    let journal: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&journals[0]).unwrap()).unwrap();
    assert_eq!(journal["projects"].as_array().unwrap().len(), 1);
    assert_eq!(journal["projects"][0]["target"], "web");
}

#[test]
#[ignore = "uses the real recycle bin"]
fn replaced_and_deleted_files_go_to_the_real_recycle_bin() {
    let setup = ReceiveSetup::new("transfer-real-bin", &[]);
    setup.incoming("deptide-test-replaced.txt", "new\n");
    setup.target("deptide-test-replaced.txt", "old\n");
    setup.target("deptide-test-deleted.txt", "gone\n");

    let result = apply_receive(
        &setup.workspace,
        &setup.config,
        &[setup.selection(
            &["deptide-test-replaced.txt"],
            &["deptide-test-deleted.txt"],
        )],
    )
    .unwrap();

    assert_eq!(result.projects[0].recycled, 2);
    assert_eq!(setup.read_target("deptide-test-replaced.txt"), "new\n");
    assert!(!setup.target_path("deptide-test-deleted.txt").exists());
}
