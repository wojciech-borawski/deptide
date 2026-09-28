mod common;

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};

use common::TempDir;
use std::ops::ControlFlow;

use deptide_core::transfer::{
    create_receive_directory, extract_virtual, read_virtual_file, sanitize_relative_path,
    top_level_folders, VirtualEntry, VirtualFolder, VirtualListing,
};

fn path(segments: &[&str]) -> PathBuf {
    segments.iter().collect()
}

/// Both receive folder tests use the real receive root under the temp folder.
fn receive_root_in_use() -> MutexGuard<'static, ()> {
    static IN_USE: Mutex<()> = Mutex::new(());
    IN_USE.lock().unwrap_or_else(PoisonError::into_inner)
}

fn entry(index: u32, segments: &[&str], is_directory: bool, size: Option<u64>) -> VirtualEntry {
    VirtualEntry {
        index,
        relative: path(segments),
        is_directory,
        size,
    }
}

#[test]
fn relative_paths_are_split_on_both_separators() {
    let index = Some(path(&["core", "src", "index.ts"]));

    assert_eq!(sanitize_relative_path("core\\src\\index.ts"), index);
    assert_eq!(sanitize_relative_path("core/src/index.ts"), index);
    assert_eq!(sanitize_relative_path("core/src\\index.ts"), index);
    assert_eq!(sanitize_relative_path("core\\\\src//index.ts\\"), index);
    assert_eq!(
        sanitize_relative_path("package.json"),
        Some(path(&["package.json"]))
    );
    assert_eq!(
        sanitize_relative_path("web\\..cache\\a..b.txt"),
        Some(path(&["web", "..cache", "a..b.txt"]))
    );
}

#[test]
fn relative_paths_that_could_leave_the_destination_are_rejected() {
    let unsafe_names = [
        "",
        "\\",
        "/",
        "\\core\\index.ts",
        "/etc/passwd",
        "C:\\Windows\\win.ini",
        "C:core\\index.ts",
        "\\\\server\\share\\index.ts",
        "//server/share/index.ts",
        "\\\\?\\C:\\index.ts",
        "..",
        "..\\index.ts",
        "core\\..\\..\\index.ts",
        "core/../index.ts",
        "core\\.. \\index.ts",
        "core\\...\\index.ts",
        "core\\.\\index.ts",
        "core\\ \\index.ts",
        "core\\index.ts:hidden",
    ];

    for name in unsafe_names {
        assert_eq!(sanitize_relative_path(name), None, "{name:?} was accepted");
    }
}

#[test]
fn names_windows_cannot_create_are_rejected() {
    let unsafe_names = [
        "web\\CON",
        "web\\con.txt",
        "web\\Prn.log",
        "aux\\index.ts",
        "web\\NUL.tar.gz",
        "web\\nul .txt",
        "web\\COM0",
        "web\\com9.ts",
        "web\\LPT1",
        "web\\lpt0.md",
        "web\\COM\u{b9}",
        "web\\lpt\u{b2}.txt",
        "web\\COM\u{b3}",
        "web\\CONIN$",
        "web\\conout$.txt",
        "web\\index.ts.",
        "web\\index.ts ",
        "web.\\index.ts",
        "web \\index.ts",
        "web\\a<b.ts",
        "web\\a>b.ts",
        "web\\a\"b.ts",
        "web\\a|b.ts",
        "web\\a?b.ts",
        "web\\a*b.ts",
        "web\\a\u{1}b.ts",
        "web\\a\tb.ts",
        "web\\a\u{1f}b.ts",
    ];

    for name in unsafe_names {
        assert_eq!(sanitize_relative_path(name), None, "{name:?} was accepted");
    }
}

#[test]
fn names_that_only_look_reserved_are_kept() {
    for name in [
        "web\\CONSOLE.txt",
        "web\\com10",
        "web\\lpt",
        "web\\nul-check.ts",
        "web\\auxiliary.ts",
        "web\\con_fig.ts",
        "web\\COM1x.ts",
        "web\\.con",
        "web\\a\u{7f}b.ts",
        "web\\ leading.ts",
    ] {
        assert!(
            sanitize_relative_path(name).is_some(),
            "{name:?} was rejected"
        );
    }
}

#[test]
fn a_file_larger_than_the_limit_is_refused_before_the_clipboard_is_read() {
    let listing = VirtualListing {
        sequence: 0,
        rejected: 0,
        entries: vec![
            entry(0, &["web", "package.json"], false, Some(2 * 1024 * 1024)),
            entry(1, &["web", "small.json"], false, Some(10)),
        ],
    };

    let error = read_virtual_file(&listing, &path(&["web", "package.json"]), 1024 * 1024)
        .expect_err("a file above the limit is refused");

    assert!(error.message().contains("larger than"), "{error}");
}

#[test]
fn top_level_folders_count_their_files_and_ignore_loose_files() {
    let listing = VirtualListing {
        sequence: 7,
        rejected: 0,
        entries: vec![
            entry(0, &["web"], true, None),
            entry(1, &["web", "package.json"], false, Some(40)),
            entry(2, &["web", "src"], true, None),
            entry(3, &["web", "src", "index.ts"], false, Some(100)),
            entry(4, &["notes.txt"], false, Some(9)),
            entry(5, &["docs", "package.json", "readme.md"], false, Some(5)),
            entry(6, &["lib", "src", "package.json"], false, Some(12)),
            entry(7, &["lib", "unsized.bin"], false, None),
            entry(8, &["empty"], true, None),
            entry(9, &["empty", "package.json"], true, None),
        ],
    };

    assert_eq!(
        top_level_folders(&listing),
        vec![
            VirtualFolder {
                name: "web".to_string(),
                files: 2,
                bytes: 140,
                has_manifest: true,
            },
            VirtualFolder {
                name: "docs".to_string(),
                files: 1,
                bytes: 5,
                has_manifest: false,
            },
            VirtualFolder {
                name: "lib".to_string(),
                files: 2,
                bytes: 12,
                has_manifest: false,
            },
            VirtualFolder {
                name: "empty".to_string(),
                files: 0,
                bytes: 0,
                has_manifest: false,
            },
        ]
    );
}

#[test]
fn extraction_refuses_a_destination_that_already_holds_files() {
    let root = TempDir::new("virtual-destination");
    let kept = root.write("existing/keep.txt", "mine\n");
    let listing = VirtualListing {
        sequence: 0,
        rejected: 0,
        entries: vec![entry(0, &["web", "a.txt"], false, Some(1))],
    };
    let mut reports = 0;

    let error = extract_virtual(&listing, &root.join("existing"), &mut |_| {
        reports += 1;
        ControlFlow::Continue(())
    })
    .expect_err("a folder with files in it is not a valid destination");

    assert!(error.message().contains("not empty"), "{error}");
    assert_eq!(fs::read_to_string(&kept).unwrap(), "mine\n");
    assert_eq!(reports, 0);
}

#[test]
fn receive_folders_are_unique_kept_apart_from_staging_and_pruned_to_five() {
    let _root = receive_root_in_use();
    let created: Vec<PathBuf> = (0..6)
        .map(|_| create_receive_directory().expect("receive folder is created"))
        .collect();

    let distinct: HashSet<&PathBuf> = created.iter().collect();
    assert_eq!(distinct.len(), created.len(), "{created:?}");

    let root = created[0].parent().unwrap().to_path_buf();
    assert_eq!(root.file_name().unwrap(), "deptide-received");
    assert_eq!(root.parent().unwrap(), std::env::temp_dir());

    assert!(!created[0].exists(), "the oldest folder was not pruned");
    for folder in &created[1..] {
        assert!(folder.is_dir(), "{} is missing", folder.display());
        assert_eq!(folder.parent().unwrap(), root);
    }

    let remaining = fs::read_dir(&root)
        .unwrap()
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .count();
    assert_eq!(remaining, 5);
}

#[test]
fn a_new_receive_folder_is_kept_when_older_folders_carry_later_names() {
    let _root = receive_root_in_use();
    let root = std::env::temp_dir().join("deptide-received");
    fs::create_dir_all(&root).unwrap();
    let later: Vec<PathBuf> = (1..=5)
        .map(|second| root.join(format!("9999-12-31T23-59-5{second}")))
        .collect();
    for folder in &later {
        fs::create_dir_all(folder).unwrap();
    }

    let created = create_receive_directory();
    let kept = created.as_ref().map(|folder| folder.is_dir());
    for folder in &later {
        let _ = fs::remove_dir_all(folder);
    }

    assert!(
        kept.expect("receive folder is created"),
        "the new folder was pruned"
    );
}
