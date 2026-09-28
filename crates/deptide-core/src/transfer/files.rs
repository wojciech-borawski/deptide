use std::fs;
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::WalkBuilder;
use sha2::{Digest, Sha256};

use crate::error::AppResult;

pub const GIT_DIRECTORY: &str = ".git";
const WHITESPACE_COMPARE_LIMIT: u64 = 5 * 1024 * 1024;
pub(super) const BINARY_SNIFF_BYTES: u64 = 8 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectedFile {
    pub absolute: PathBuf,
    pub relative: PathBuf,
    pub size: u64,
}

#[derive(Debug, Default, Clone)]
pub struct Collection {
    pub files: Vec<CollectedFile>,
    pub skipped: usize,
}

impl Collection {
    pub fn bytes(&self) -> u64 {
        self.files.iter().map(|file| file.size).sum()
    }
}

fn extra_rules(root: &Path, patterns: &[String]) -> Option<Gitignore> {
    let mut builder = GitignoreBuilder::new(root);
    let mut added = false;

    for pattern in patterns {
        let trimmed = pattern.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if builder.add_line(None, trimmed).is_ok() {
            added = true;
        }
    }

    added.then(|| builder.build().ok()).flatten()
}

pub fn collect_files(root: &Path, patterns: &[String], respect_gitignore: bool) -> Collection {
    walk(root, patterns, respect_gitignore, true)
}

/// `collect_files` without the ignore files above `root`, which belong to the folder it was received into.
pub fn collect_received_files(root: &Path, patterns: &[String]) -> Collection {
    walk(root, patterns, true, false)
}

fn walk(root: &Path, patterns: &[String], respect_gitignore: bool, parents: bool) -> Collection {
    let rules = extra_rules(root, patterns);
    let mut collection = Collection::default();

    let walker = WalkBuilder::new(root)
        .hidden(false)
        .parents(parents)
        .git_ignore(respect_gitignore)
        .git_global(respect_gitignore)
        .git_exclude(respect_gitignore)
        .require_git(false)
        .filter_entry(|entry| entry.file_name() != GIT_DIRECTORY)
        .build();

    for entry in walker.flatten() {
        let is_file = entry.file_type().is_some_and(|kind| kind.is_file());
        if !is_file {
            continue;
        }

        let absolute = entry.path().to_path_buf();
        let Ok(relative) = absolute.strip_prefix(root).map(Path::to_path_buf) else {
            continue;
        };

        let ignored = rules.as_ref().is_some_and(|rules| {
            rules
                .matched_path_or_any_parents(&relative, false)
                .is_ignore()
        });
        if ignored {
            collection.skipped += 1;
            continue;
        }

        let size = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
        collection.files.push(CollectedFile {
            absolute,
            relative,
            size,
        });
    }

    collection.files.sort_by(|a, b| a.relative.cmp(&b.relative));
    collection
}

pub fn copy_file(source: &Path, target: &Path) -> AppResult<u64> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(fs::copy(source, target)?)
}

pub fn copy_collection(collection: &Collection, target_root: &Path) -> AppResult<u64> {
    let mut bytes = 0;

    for file in &collection.files {
        bytes += copy_file(&file.absolute, &target_root.join(&file.relative))?;
    }

    Ok(bytes)
}

pub fn same_content(a: &Path, b: &Path) -> bool {
    let (Ok(meta_a), Ok(meta_b)) = (fs::metadata(a), fs::metadata(b)) else {
        return false;
    };
    if meta_a.len() != meta_b.len() {
        return false;
    }

    match (digest(a), digest(b)) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

fn digest(path: &Path) -> Option<Vec<u8>> {
    let mut file = fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut hasher).ok()?;
    Some(hasher.finalize().to_vec())
}

/// Whether both files hold the same text once line endings (CRLF, LF, CR),
/// trailing whitespace and blank lines are ignored. Indentation still counts.
/// Always false for a file over 5 MiB or with a NUL byte in its first 8 KiB.
pub fn same_ignoring_whitespace(a: &Path, b: &Path) -> bool {
    is_comparable_text(a) && is_comparable_text(b) && same_text_lines(a, b).unwrap_or(false)
}

fn is_comparable_text(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() || meta.len() > WHITESPACE_COMPARE_LIMIT {
        return false;
    }

    let mut head = Vec::new();
    fs::File::open(path)
        .and_then(|file| file.take(BINARY_SNIFF_BYTES).read_to_end(&mut head))
        .is_ok_and(|_| !head.contains(&0))
}

fn same_text_lines(a: &Path, b: &Path) -> io::Result<bool> {
    let mut left = TextLines::open(a)?;
    let mut right = TextLines::open(b)?;

    loop {
        let (left_line, right_line) = (left.next_non_blank()?, right.next_non_blank()?);
        if left_line != right_line {
            return Ok(false);
        }
        if left_line.is_none() {
            return Ok(true);
        }
    }
}

struct TextLines {
    reader: BufReader<fs::File>,
    line: Vec<u8>,
}

impl TextLines {
    fn open(path: &Path) -> io::Result<Self> {
        Ok(Self {
            reader: BufReader::new(fs::File::open(path)?),
            line: Vec::new(),
        })
    }

    /// The next line that is not blank, without its trailing whitespace.
    fn next_non_blank(&mut self) -> io::Result<Option<&[u8]>> {
        loop {
            self.line.clear();
            if !self.read_until_line_break()? {
                return Ok(None);
            }
            let end = self
                .line
                .iter()
                .rposition(|byte| !byte.is_ascii_whitespace())
                .map_or(0, |last| last + 1);
            if end > 0 {
                return Ok(Some(&self.line[..end]));
            }
        }
    }

    fn read_until_line_break(&mut self) -> io::Result<bool> {
        loop {
            let available = self.reader.fill_buf()?;
            if available.is_empty() {
                return Ok(!self.line.is_empty());
            }
            if let Some(index) = available
                .iter()
                .position(|byte| matches!(byte, b'\r' | b'\n'))
            {
                self.line.extend_from_slice(&available[..index]);
                self.reader.consume(index + 1);
                return Ok(true);
            }
            let length = available.len();
            self.line.extend_from_slice(available);
            self.reader.consume(length);
        }
    }
}

pub fn to_posix(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
