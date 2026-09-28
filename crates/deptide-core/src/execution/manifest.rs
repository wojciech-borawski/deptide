use std::cmp::Ordering;
use std::fs;
use std::io;
use std::path::Path;
use std::thread;
use std::time::Duration;

use serde::Serialize;
use serde_json::ser::{PrettyFormatter, Serializer};
use serde_json::{Map, Value};

use super::steps::ManifestEntry;
use crate::error::{AppError, AppResult};
use crate::scan::MANIFEST_FILE_NAME;

const BOM: char = '\u{feff}';
const TEMPORARY_SUFFIX: &str = ".deptide-restore";
const REPLACE_ATTEMPTS: u32 = 10;
const REPLACE_PAUSE: Duration = Duration::from_millis(100);
const SHARING_VIOLATION: i32 = 32;
const PUNCTUATION_ORDER: &str = r#"_-,;:!?.'"()[]{}@*/\&#%`^+<=>|~$"#;

struct Layout {
    bom: bool,
    indent: Option<String>,
    crlf: bool,
    final_newline: bool,
}

impl Layout {
    /// Reads the indent the way npm does: the whitespace on the line after the opening brace, none when the brace is not followed by a line break.
    fn of(text: &str) -> Self {
        let body = text.trim_start_matches(BOM).trim_start();
        let after_brace = body.strip_prefix('{').unwrap_or(body);
        let next_line = after_brace.trim_start_matches(['\r', '\n']);
        let indent = (next_line.len() < after_brace.len())
            .then(|| next_line.trim_start_matches([' ', '\t']))
            .map(|rest| &next_line[..next_line.len() - rest.len()])
            .filter(|indent| !indent.is_empty())
            .map(str::to_string);

        Self {
            bom: text.starts_with(BOM),
            indent,
            crlf: text.contains("\r\n"),
            final_newline: text.ends_with('\n'),
        }
    }

    fn render(&self, manifest: &Value) -> AppResult<String> {
        let mut output = Vec::new();
        match &self.indent {
            Some(indent) => {
                let formatter = PrettyFormatter::with_indent(indent.as_bytes());
                manifest.serialize(&mut Serializer::with_formatter(&mut output, formatter))?;
            }
            None => serde_json::to_writer(&mut output, manifest)?,
        }

        let mut text = String::from_utf8_lossy(&output).into_owned();
        if self.final_newline {
            text.push('\n');
        }
        if self.crlf {
            text = text.replace('\n', "\r\n");
        }
        if self.bom {
            text.insert(0, BOM);
        }
        Ok(text)
    }
}

fn primary_weight(character: char) -> (u8, u32) {
    if let Some(position) = PUNCTUATION_ORDER.find(character) {
        (0, position as u32)
    } else if character.is_ascii_digit() {
        (1, character as u32)
    } else {
        (
            2,
            character.to_lowercase().next().unwrap_or(character) as u32,
        )
    }
}

/// Orders dependency names the way npm does (`localeCompare(name, "en")`) for the names npm allows.
fn compare_names(left: &str, right: &str) -> Ordering {
    left.chars()
        .map(primary_weight)
        .cmp(right.chars().map(primary_weight))
        .then_with(|| {
            left.chars()
                .map(char::is_uppercase)
                .cmp(right.chars().map(char::is_uppercase))
        })
        .then_with(|| left.cmp(right))
}

fn sorted(section: Map<String, Value>) -> Map<String, Value> {
    let mut entries: Vec<(String, Value)> = section.into_iter().collect();
    entries.sort_by(|(left, _), (right, _)| compare_names(left, right));
    entries.into_iter().collect()
}

fn set_entries(manifest: &mut Value, entries: &[ManifestEntry]) -> AppResult<()> {
    let object = manifest
        .as_object_mut()
        .ok_or_else(|| AppError::new("package.json is not a JSON object"))?;

    for entry in entries {
        let section = object
            .entry(entry.field())
            .or_insert_with(|| Value::Object(Map::new()));
        let Value::Object(map) = section else {
            return Err(AppError::new(format!(
                "{} in package.json is not an object",
                entry.field()
            )));
        };
        map.insert(entry.name.clone(), Value::String(entry.range.clone()));
        *map = sorted(std::mem::take(map));
    }
    Ok(())
}

fn is_busy(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::PermissionDenied
        || (cfg!(windows) && error.raw_os_error() == Some(SHARING_VIOLATION))
}

fn replace(temporary: &Path, path: &Path, text: &str) -> io::Result<()> {
    for attempt in 1..=REPLACE_ATTEMPTS {
        match fs::rename(temporary, path) {
            Ok(()) => return Ok(()),
            Err(error) if is_busy(&error) && attempt < REPLACE_ATTEMPTS => {
                thread::sleep(REPLACE_PAUSE);
            }
            Err(_) => break,
        }
    }
    fs::write(path, text)
}

/// Sets `entries` in the project's package.json as npm would write them: dependency sections sorted, indentation, line endings and BOM kept.
pub fn write_entries(directory: &Path, entries: &[ManifestEntry]) -> AppResult<()> {
    let path = directory.join(MANIFEST_FILE_NAME);
    let text = fs::read_to_string(&path)
        .map_err(|error| AppError::with_context("could not read package.json", error))?;
    let layout = Layout::of(&text);

    let mut manifest: Value = serde_json::from_str(text.trim_start_matches(BOM))
        .map_err(|error| AppError::with_context("package.json is not valid JSON", error))?;
    set_entries(&mut manifest, entries)?;
    let rendered = layout.render(&manifest)?;

    let temporary_name = format!("{MANIFEST_FILE_NAME}{TEMPORARY_SUFFIX}");
    let temporary = directory.join(&temporary_name);
    let written =
        fs::write(&temporary, &rendered).and_then(|()| replace(&temporary, &path, &rendered));
    let Err(error) = written else {
        let _ = fs::remove_file(&temporary);
        return Ok(());
    };

    if fs::read_to_string(&path).is_ok_and(|current| current == text)
        || fs::write(&path, &text).is_ok()
    {
        let _ = fs::remove_file(&temporary);
        return Err(AppError::with_context(
            "could not write package.json",
            error,
        ));
    }
    Err(AppError::with_context(
        &format!("could not write package.json, the new content is in {temporary_name}"),
        error,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::DependencySection;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

    fn entry(section: DependencySection, name: &str, range: &str) -> ManifestEntry {
        ManifestEntry {
            section,
            name: name.to_string(),
            range: range.to_string(),
        }
    }

    static FOLDERS: AtomicUsize = AtomicUsize::new(0);

    fn temporary_folder() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "deptide-manifest-{}-{}",
            std::process::id(),
            FOLDERS.fetch_add(1, AtomicOrdering::Relaxed)
        ))
    }

    fn written(original: &str, entries: &[ManifestEntry]) -> String {
        let directory = temporary_folder();
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join(MANIFEST_FILE_NAME), original).unwrap();

        let result = write_entries(&directory, entries);
        let text = fs::read_to_string(directory.join(MANIFEST_FILE_NAME)).unwrap();
        let leftovers = fs::read_dir(&directory).unwrap().count();
        fs::remove_dir_all(&directory).unwrap();

        result.unwrap();
        assert_eq!(leftovers, 1, "only package.json is left in the folder");
        text
    }

    #[test]
    fn a_peer_range_is_set_without_touching_the_dependency_range() {
        let original = "{\n  \"name\": \"lib\",\n  \"dependencies\": {\n    \"left-pad\": \"1.3.0\"\n  },\n  \"peerDependencies\": {\n    \"left-pad\": \"^1.3.0\"\n  }\n}\n";

        let text = written(
            original,
            &[entry(DependencySection::Peer, "left-pad", "^1.0.0")],
        );

        assert_eq!(
            text,
            "{\n  \"name\": \"lib\",\n  \"dependencies\": {\n    \"left-pad\": \"1.3.0\"\n  },\n  \"peerDependencies\": {\n    \"left-pad\": \"^1.0.0\"\n  }\n}\n"
        );
    }

    #[test]
    fn dependency_sections_are_sorted_like_npm_and_other_keys_keep_their_order() {
        let original = "{\n  \"version\": \"1.0.0\",\n  \"name\": \"lib\",\n  \"dependencies\": {\n    \"axios\": \"1.0.0\",\n    \"zod\": \"3.0.0\"\n  }\n}\n";

        let text = written(
            original,
            &[
                entry(DependencySection::Dependencies, "zod", "3.1.0"),
                entry(DependencySection::Dependencies, "left-pad", "1.3.0"),
                entry(DependencySection::Peer, "zod", "^3.0.0"),
            ],
        );

        assert_eq!(
            text,
            "{\n  \"version\": \"1.0.0\",\n  \"name\": \"lib\",\n  \"dependencies\": {\n    \"axios\": \"1.0.0\",\n    \"left-pad\": \"1.3.0\",\n    \"zod\": \"3.1.0\"\n  },\n  \"peerDependencies\": {\n    \"zod\": \"^3.0.0\"\n  }\n}\n"
        );
    }

    #[test]
    fn names_sort_in_the_order_node_gives_for_locale_compare_en() {
        let expected = [
            "_x", "@a/z", "@scope/b", "1x", "A", "a-b", "a.b", "left_pad", "left-pad", "Left-pad",
            "leftpad", "zod",
        ];
        let mut names = expected;
        names.reverse();

        names.sort_by(|left, right| compare_names(left, right));

        assert_eq!(names, expected);
    }

    #[test]
    fn indentation_line_endings_bom_and_final_newline_are_kept() {
        let cases = [
            (
                "{\n    \"peerDependencies\": {\n        \"a\": \"1\"\n    }\n}\n",
                "{\n    \"peerDependencies\": {\n        \"a\": \"2\"\n    }\n}\n",
            ),
            (
                "{\n\t\"peerDependencies\": {\n\t\t\"a\": \"1\"\n\t}\n}",
                "{\n\t\"peerDependencies\": {\n\t\t\"a\": \"2\"\n\t}\n}",
            ),
            (
                "\u{feff}{\r\n  \"peerDependencies\": {\r\n    \"a\": \"1\"\r\n  }\r\n}\r\n",
                "\u{feff}{\r\n  \"peerDependencies\": {\r\n    \"a\": \"2\"\r\n  }\r\n}\r\n",
            ),
            (
                "{\"peerDependencies\":{\"a\":\"1\"}}",
                "{\"peerDependencies\":{\"a\":\"2\"}}",
            ),
            (
                "{\"peerDependencies\":{\"a\":\"1\"}}\n",
                "{\"peerDependencies\":{\"a\":\"2\"}}\n",
            ),
            (
                "\u{feff}{\"peerDependencies\":{\"a\":\"1\"}}\r\n",
                "\u{feff}{\"peerDependencies\":{\"a\":\"2\"}}\r\n",
            ),
        ];

        for (original, expected) in cases {
            let text = written(original, &[entry(DependencySection::Peer, "a", "2")]);
            assert_eq!(text, expected, "original: {original:?}");
        }
    }

    #[test]
    fn other_values_are_written_back_as_they_were() {
        let original = "{\n  \"name\": \"lib\",\n  \"files\": [],\n  \"config\": {},\n  \"weight\": 1.5,\n  \"big\": 12345678901234567890,\n  \"text\": \"a\\\"b\\\\c\\n\u{e9}/\",\n  \"devDependencies\": {\n    \"a\": \"1\"\n  }\n}\n";

        let text = written(original, &[entry(DependencySection::Dev, "a", "2")]);

        assert_eq!(text, original.replace("\"a\": \"1\"", "\"a\": \"2\""));
    }

    fn refused(original: &str) -> (String, String) {
        let directory = temporary_folder();
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join(MANIFEST_FILE_NAME), original).unwrap();

        let result = write_entries(&directory, &[entry(DependencySection::Peer, "a", "1")]);
        let text = fs::read_to_string(directory.join(MANIFEST_FILE_NAME)).unwrap();
        let leftovers = fs::read_dir(&directory).unwrap().count();
        fs::remove_dir_all(&directory).unwrap();

        assert_eq!(leftovers, 1, "only package.json is left in the folder");
        (result.unwrap_err().to_string(), text)
    }

    #[test]
    fn a_manifest_that_is_not_valid_json_is_left_alone() {
        let (error, text) = refused("{ broken");

        assert!(
            error.starts_with("package.json is not valid JSON"),
            "{error}"
        );
        assert_eq!(text, "{ broken");
    }

    #[test]
    fn a_manifest_or_section_that_is_not_an_object_is_left_alone() {
        for (original, expected) in [
            ("[]", "package.json is not a JSON object"),
            (
                "{\"peerDependencies\": []}",
                "peerDependencies in package.json is not an object",
            ),
        ] {
            let (error, text) = refused(original);

            assert_eq!(error, expected);
            assert_eq!(text, original);
        }
    }

    #[cfg(windows)]
    #[test]
    fn a_manifest_held_open_without_delete_sharing_is_still_written() {
        use std::os::windows::fs::OpenOptionsExt;

        const FILE_SHARE_READ: u32 = 0x1;
        const FILE_SHARE_WRITE: u32 = 0x2;

        let directory = temporary_folder();
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join(MANIFEST_FILE_NAME);
        fs::write(
            &path,
            "{\n  \"peerDependencies\": {\n    \"a\": \"1\"\n  }\n}\n",
        )
        .unwrap();
        let holder = fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .open(&path)
            .unwrap();

        let result = write_entries(&directory, &[entry(DependencySection::Peer, "a", "2")]);
        drop(holder);
        let text = fs::read_to_string(&path).unwrap();
        let leftovers = fs::read_dir(&directory).unwrap().count();
        fs::remove_dir_all(&directory).unwrap();

        result.unwrap();
        assert_eq!(
            text,
            "{\n  \"peerDependencies\": {\n    \"a\": \"2\"\n  }\n}\n"
        );
        assert_eq!(leftovers, 1, "only package.json is left in the folder");
    }

    #[cfg(windows)]
    #[test]
    fn a_manifest_locked_against_writing_is_left_as_it_was() {
        use std::os::windows::fs::OpenOptionsExt;

        const FILE_SHARE_READ: u32 = 0x1;

        let original = "{\n  \"peerDependencies\": {\n    \"a\": \"1\"\n  }\n}\n";
        let directory = temporary_folder();
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join(MANIFEST_FILE_NAME);
        fs::write(&path, original).unwrap();
        let holder = fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(&path)
            .unwrap();

        let result = write_entries(&directory, &[entry(DependencySection::Peer, "a", "2")]);
        drop(holder);
        let text = fs::read_to_string(&path).unwrap();
        let leftovers = fs::read_dir(&directory).unwrap().count();
        fs::remove_dir_all(&directory).unwrap();

        let error = result.unwrap_err().to_string();
        assert!(
            error.starts_with("could not write package.json: "),
            "{error}"
        );
        assert_eq!(text, original);
        assert_eq!(leftovers, 1, "only package.json is left in the folder");
    }
}
