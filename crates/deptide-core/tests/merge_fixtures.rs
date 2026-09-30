use std::fs;
use std::path::{Path, PathBuf};

use deptide_core::transfer::{merge_lines, LineChunk};
use serde::Deserialize;

const BOM: &str = "\u{FEFF}";

#[derive(Deserialize)]
struct Fixture {
    name: String,
    local: String,
    received: String,
    /// Every chunk of the line diff from `local` to `received`.
    chunks: Vec<LineChunk>,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    take: Vec<usize>,
    expected: String,
}

fn fixtures() -> Vec<(PathBuf, Fixture)> {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/merge");
    let mut found: Vec<(PathBuf, Fixture)> = fs::read_dir(&folder)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .map(|path| {
            let fixture = serde_json::from_str(&fs::read_to_string(&path).unwrap())
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            (path, fixture)
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

fn merge(fixture: &Fixture, chunks: &[LineChunk]) -> String {
    let bytes = merge_lines(
        fixture.local.as_bytes(),
        fixture.received.as_bytes(),
        chunks,
    )
    .unwrap_or_else(|error| panic!("{}: {error:?}", fixture.name));
    String::from_utf8(bytes).unwrap()
}

#[test]
fn every_fixture_merges_to_its_expected_text() {
    let fixtures = fixtures();
    assert!(fixtures.len() >= 9, "found {} fixtures", fixtures.len());

    for (path, fixture) in &fixtures {
        assert!(!fixture.cases.is_empty(), "{}", path.display());
        for case in &fixture.cases {
            let chunks: Vec<LineChunk> = case
                .take
                .iter()
                .map(|&index| fixture.chunks[index])
                .collect();
            assert_eq!(
                merge(fixture, &chunks),
                case.expected,
                "{} taking {:?}",
                path.display(),
                case.take
            );
        }
    }
}

#[test]
fn every_fixture_lists_all_chunks_of_its_diff() {
    for (path, fixture) in fixtures() {
        let local_bom = if fixture.local.starts_with(BOM) {
            BOM
        } else {
            ""
        };
        let received = fixture
            .received
            .strip_prefix(BOM)
            .unwrap_or(&fixture.received);

        assert_eq!(
            merge(&fixture, &fixture.chunks),
            format!("{local_bom}{received}"),
            "{}",
            path.display()
        );
        assert_eq!(merge(&fixture, &[]), fixture.local, "{}", path.display());
    }
}
