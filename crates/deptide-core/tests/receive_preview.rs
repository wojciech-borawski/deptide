mod common;

use std::fs;

use common::TempDir;
use deptide_core::domain::{ConfiguredProject, UpdateConfig};
use deptide_core::transfer::{read_receive_file, FileSide, ReceiveFileContents, PREVIEW_LIMIT};
use deptide_core::workspace::Workspace;
use sha2::{Digest, Sha256};

/// A workspace with one project `web` in `repos/web` and a received copy of
/// it in `incoming/web`.
struct PreviewSetup {
    root: TempDir,
    workspace: Workspace,
    config: UpdateConfig,
}

impl PreviewSetup {
    fn new(label: &str) -> Self {
        let root = TempDir::new(label);
        root.mkdir("incoming/web");
        root.mkdir("repos/web");
        let tool = root.mkdir("tool");
        let workspace = Workspace::open(&tool).unwrap();
        let config = UpdateConfig {
            projects: vec![ConfiguredProject::new("web", "../repos/web")],
            ..UpdateConfig::default()
        };
        Self {
            root,
            workspace,
            config,
        }
    }

    fn incoming(&self, relative: &str, bytes: &[u8]) {
        self.write(&format!("incoming/web/{relative}"), bytes);
    }

    fn target(&self, relative: &str, bytes: &[u8]) {
        self.write(&format!("repos/web/{relative}"), bytes);
    }

    fn write(&self, relative: &str, bytes: &[u8]) {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn read(&self, relative: &str) -> Result<ReceiveFileContents, String> {
        read_receive_file(
            &self.workspace,
            &self.config,
            &self.root.join("incoming/web").to_string_lossy(),
            "web",
            relative,
        )
        .map_err(|error| error.to_string())
    }
}

/// A text side whose raw bytes are `text`, after a BOM when `bom` is set.
fn text(text: &str, bom: bool, size: u64) -> Option<FileSide> {
    let raw = [if bom { "\u{FEFF}" } else { "" }, text].concat();
    Some(FileSide::Text {
        text: text.to_string(),
        bom,
        size,
        sha256: format!("{:x}", Sha256::digest(raw.as_bytes())),
        utf8: true,
    })
}

#[test]
fn both_sides_are_read_as_text_with_the_bom_stripped_and_reported() {
    let setup = PreviewSetup::new("preview-text");
    setup.incoming("src/index.ts", b"\xEF\xBB\xBFexport const a = 2;\r\n");
    setup.target("src/index.ts", b"export const a = 1;\n");

    let contents = setup.read("src/index.ts").unwrap();

    assert_eq!(contents.received, text("export const a = 2;\r\n", true, 24));
    assert_eq!(contents.local, text("export const a = 1;\n", false, 20));
}

#[test]
fn the_hash_covers_the_whole_file_with_its_bom() {
    let setup = PreviewSetup::new("preview-hash");
    setup.incoming("abc.txt", b"abc");
    setup.target("abc.txt", b"\xEF\xBB\xBFabc");

    let contents = setup.read("abc.txt").unwrap();

    let Some(FileSide::Text { sha256, .. }) = &contents.received else {
        panic!("{contents:?}");
    };
    assert_eq!(
        sha256,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    let Some(FileSide::Text { sha256, .. }) = &contents.local else {
        panic!("{contents:?}");
    };
    assert_eq!(
        sha256,
        "1c28dc3f1f804a1ad9c9b4b4cf5e2658d16ad4ed08e3020d04a8d2865018947c"
    );
}

#[test]
fn invalid_utf8_is_decoded_with_replacement_characters() {
    let setup = PreviewSetup::new("preview-lossy");
    setup.incoming("latin1.txt", b"caf\xE9\n");
    setup.target("latin1.txt", b"\xEF\xBB\xBFcaf\xE9\n");

    let contents = setup.read("latin1.txt").unwrap();

    assert_eq!(
        contents.received,
        Some(FileSide::Text {
            text: "caf\u{FFFD}\n".to_string(),
            bom: false,
            size: 5,
            sha256: format!("{:x}", Sha256::digest(b"caf\xE9\n")),
            utf8: false,
        })
    );
    assert!(matches!(
        contents.local,
        Some(FileSide::Text {
            bom: true,
            utf8: false,
            ..
        })
    ));
}

#[test]
fn a_missing_side_is_none() {
    let setup = PreviewSetup::new("preview-missing");
    setup.incoming("src/new.ts", b"new\n");
    setup.target("src/old.ts", b"old\n");
    setup.target("src/folder/inner.ts", b"inner\n");
    setup.incoming("src/folder", b"a file where the target has a folder\n");

    let added = setup.read("src/new.ts").unwrap();
    let removed = setup.read("src/old.ts").unwrap();
    let folder = setup.read("src/folder").unwrap();
    let neither = setup.read("src/nowhere.ts").unwrap();

    assert_eq!(added.received, text("new\n", false, 4));
    assert_eq!(added.local, None);
    assert_eq!(removed.received, None);
    assert_eq!(removed.local, text("old\n", false, 4));
    assert!(matches!(folder.received, Some(FileSide::Text { .. })));
    assert_eq!(folder.local, None);
    assert_eq!(neither.received, None);
    assert_eq!(neither.local, None);
}

#[test]
fn a_nul_byte_in_the_first_8_kib_makes_a_file_binary() {
    let setup = PreviewSetup::new("preview-binary");
    setup.incoming("logo.png", b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR");
    let mut late = vec![b'x'; 8 * 1024];
    late.extend_from_slice(b"\0\n");
    setup.target("late.txt", &late);

    let early = setup.read("logo.png").unwrap();
    let late_nul = setup.read("late.txt").unwrap();

    assert_eq!(early.received, Some(FileSide::Binary { size: 16 }));
    assert!(matches!(
        late_nul.local,
        Some(FileSide::Text { size: 8194, .. })
    ));
}

#[test]
fn files_over_one_mib_are_too_large_unless_they_are_binary() {
    let setup = PreviewSetup::new("preview-limit");
    let limit = usize::try_from(PREVIEW_LIMIT).unwrap();
    assert_eq!(limit, 1024 * 1024);
    setup.incoming("at-limit.txt", &vec![b'a'; limit]);
    setup.incoming("over-limit.txt", &vec![b'a'; limit + 1]);
    let mut big_binary = vec![0u8; 16];
    big_binary.extend(vec![b'a'; 2 * limit]);
    setup.incoming("big.bin", &big_binary);

    let at_limit = setup.read("at-limit.txt").unwrap().received;
    let over_limit = setup.read("over-limit.txt").unwrap().received;
    let binary = setup.read("big.bin").unwrap().received;

    assert!(matches!(at_limit, Some(FileSide::Text { size, .. }) if size == PREVIEW_LIMIT));
    assert_eq!(
        over_limit,
        Some(FileSide::TooLarge {
            size: PREVIEW_LIMIT + 1
        })
    );
    assert_eq!(
        binary,
        Some(FileSide::Binary {
            size: 2 * PREVIEW_LIMIT + 16
        })
    );
}

#[test]
fn paths_that_could_leave_the_project_are_rejected() {
    let setup = PreviewSetup::new("preview-paths");
    setup.write("incoming/secret.txt", b"outside\n");
    setup.write("repos/secret.txt", b"outside\n");
    setup.target(".git/HEAD", b"ref: refs/heads/main\n");

    let mut rejected = vec![
        "",
        "../secret.txt",
        "src/../../secret.txt",
        "..\\secret.txt",
        "/etc/passwd",
        "\\secret.txt",
        "\\\\server\\share\\secret.txt",
        "//server/share/secret.txt",
        ".git/HEAD",
        "src//index.ts",
    ];
    if cfg!(windows) {
        rejected.extend(["C:\\Windows\\win.ini", "C:secret.txt", "secret.txt."]);
    }

    for relative in rejected {
        let result = setup.read(relative);
        assert!(result.is_err(), "{relative:?} was accepted: {result:?}");
    }
}

#[test]
fn an_unknown_target_or_a_missing_source_folder_is_an_error() {
    let setup = PreviewSetup::new("preview-unknown");
    let source = setup
        .root
        .join("incoming/web")
        .to_string_lossy()
        .to_string();
    let gone = setup
        .root
        .join("incoming/gone")
        .to_string_lossy()
        .to_string();

    let unknown = read_receive_file(&setup.workspace, &setup.config, &source, "api", "a.ts");
    let missing = read_receive_file(&setup.workspace, &setup.config, &gone, "web", "a.ts");

    assert!(unknown
        .unwrap_err()
        .to_string()
        .contains("Unknown project api"));
    assert!(missing
        .unwrap_err()
        .to_string()
        .contains("Source folder is gone"));
}

#[test]
fn file_sides_serialize_with_a_kind_tag() {
    let contents = ReceiveFileContents {
        received: text("a", true, 4),
        local: Some(FileSide::TooLarge { size: 9 }),
    };

    let json = serde_json::to_value(&contents).unwrap();

    assert_eq!(
        json,
        serde_json::json!({
            "received": {
                "kind": "text",
                "text": "a",
                "bom": true,
                "size": 4,
                "sha256": "1951c7860e968e742658b3af34e60741eb4aaf2a8d2ecc3993727016b12e81e8",
                "utf8": true,
            },
            "local": { "kind": "tooLarge", "size": 9 },
        })
    );
    assert_eq!(
        serde_json::to_value(FileSide::Binary { size: 3 }).unwrap(),
        serde_json::json!({ "kind": "binary", "size": 3 })
    );
}
