#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub fn new(label: &str) -> Self {
        let sequence = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "deptide-test-{label}-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temp dir can be created");
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn join(&self, relative: &str) -> PathBuf {
        self.path.join(relative)
    }

    pub fn write(&self, relative: &str, content: &str) -> PathBuf {
        let target = self.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).expect("parent can be created");
        }
        fs::write(&target, content).expect("file can be written");
        target
    }

    pub fn mkdir(&self, relative: &str) -> PathBuf {
        let target = self.join(relative);
        fs::create_dir_all(&target).expect("dir can be created");
        target
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub fn manifest(name: &str, version: &str, dependencies: &[(&str, &str)]) -> String {
    let deps: Vec<String> = dependencies
        .iter()
        .map(|(dep, range)| format!("\"{dep}\": \"{range}\""))
        .collect();

    format!(
        "{{\"name\": \"{name}\", \"version\": \"{version}\", \"scripts\": {{\"build\": \"echo build\"}}, \"dependencies\": {{{}}}}}",
        deps.join(", ")
    )
}
