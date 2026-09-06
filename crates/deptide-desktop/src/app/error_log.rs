use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use chrono::Utc;

use deptide_core::util::time::iso_timestamp;

const ERROR_LOG_FILE_NAME: &str = "deptide-errors.log";

static ERROR_LOG: OnceLock<PathBuf> = OnceLock::new();

pub fn install(log_directory: &Path) {
    let _ = fs::create_dir_all(log_directory);
    let _ = ERROR_LOG.set(log_directory.join(ERROR_LOG_FILE_NAME));

    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|location| format!(" at {}:{}", location.file(), location.line()))
            .unwrap_or_default();
        record("panic", &format!("{info}{location}"));
        previous(info);
    }));
}

pub fn path() -> Option<PathBuf> {
    ERROR_LOG.get().cloned()
}

pub fn record(source: &str, message: &str) {
    let Some(path) = ERROR_LOG.get() else {
        return;
    };

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(
            file,
            "{} [{source}] {}",
            iso_timestamp(Utc::now()),
            message.trim().replace('\n', "\n    ")
        );
    }
}
