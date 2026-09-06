use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::WalkBuilder;
use sha2::{Digest, Sha256};

use crate::error::AppResult;

const GIT_DIRECTORY: &str = ".git";

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
    let rules = extra_rules(root, patterns);
    let mut collection = Collection::default();

    let walker = WalkBuilder::new(root)
        .hidden(false)
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

pub fn to_posix(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
