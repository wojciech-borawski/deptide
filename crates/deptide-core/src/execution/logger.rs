use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, Utc};

use crate::domain::RunSummary;
use crate::error::AppResult;
use crate::util::json::{read_json, write_json};
use crate::util::text::slugify;
use crate::util::time::{file_stamp, format_duration, iso_timestamp};

const LOG_EXTENSION: &str = "log";
const SUMMARY_EXTENSION: &str = "json";
const MAX_STORED_LOGS: usize = 50;

pub struct RunLogger {
    file: Mutex<File>,
    run_name: String,
    log_file: PathBuf,
    summary_file: PathBuf,
}

pub(crate) fn stamped_run_name(moment: DateTime<Utc>, label: &str) -> String {
    let suffix = slugify(label);
    if suffix.is_empty() {
        file_stamp(moment)
    } else {
        format!("{}-{suffix}", file_stamp(moment))
    }
}

fn claim_run_name(logs_directory: &Path, base: &str) -> AppResult<(String, File)> {
    let mut sequence = 1;
    loop {
        let name = if sequence == 1 {
            base.to_string()
        } else {
            format!("{base}-{sequence}")
        };
        let claimed = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(logs_directory.join(format!("{name}.{LOG_EXTENSION}")));

        match claimed {
            Ok(file) => return Ok((name, file)),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => sequence += 1,
            Err(error) => return Err(error.into()),
        }
    }
}

impl RunLogger {
    pub fn open(logs_directory: &Path, label: &str, header: &[String]) -> AppResult<Self> {
        fs::create_dir_all(logs_directory)?;

        let started_at = Utc::now();
        let (run_name, file) =
            claim_run_name(logs_directory, &stamped_run_name(started_at, label))?;
        let log_file = logs_directory.join(format!("{run_name}.{LOG_EXTENSION}"));
        let summary_file = logs_directory.join(format!("{run_name}.{SUMMARY_EXTENSION}"));

        let logger = Self {
            file: Mutex::new(file),
            run_name,
            log_file,
            summary_file,
        };

        logger.note(&format!("deptide run {}", iso_timestamp(started_at)));
        for line in header {
            logger.note(line);
        }
        logger.note("");

        Ok(logger)
    }

    pub fn run_name(&self) -> &str {
        &self.run_name
    }

    pub fn log_file(&self) -> &Path {
        &self.log_file
    }

    pub fn write(&self, job_name: &str, line: &str) {
        self.note(&format!("[{job_name}] {line}"));
    }

    pub fn note(&self, line: &str) {
        if let Ok(mut file) = self.file.lock() {
            let _ = writeln!(file, "{line}");
        }
    }

    pub fn close(&self, summary: &RunSummary) -> AppResult<()> {
        self.note("");
        self.note("summary");

        for project in &summary.projects {
            let detail = if !project.error.is_empty() {
                format!(" - {}", project.error)
            } else if !project.warning.is_empty() {
                format!(" - {}", project.warning)
            } else {
                String::new()
            };

            self.note(&format!(
                "  {:<8}{} {}{detail}",
                format!("{:?}", project.status).to_lowercase(),
                project.name,
                format_duration(project.duration_ms)
            ));
        }

        self.note(&format!(
            "result: {} ok, {} warn, {} failed, {} skipped",
            summary.ok_count, summary.warn_count, summary.failed_count, summary.skipped_count
        ));
        self.note(&format!(
            "total time: {} (projects busy for {})",
            format_duration(summary.total_duration_ms),
            format_duration(summary.busy_duration_ms)
        ));

        if let Ok(mut file) = self.file.lock() {
            let _ = file.flush();
        }

        write_json(&self.summary_file, summary)?;
        prune_old_logs(self.log_file.parent().unwrap_or(Path::new(".")));

        Ok(())
    }
}

fn prune_old_logs(logs_directory: &Path) {
    let Ok(entries) = fs::read_dir(logs_directory) else {
        return;
    };

    let mut logs: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == LOG_EXTENSION))
        .collect();

    logs.sort();
    logs.reverse();

    for log in logs.into_iter().skip(MAX_STORED_LOGS) {
        let _ = fs::remove_file(&log);
        let _ = fs::remove_file(log.with_extension(SUMMARY_EXTENSION));
    }
}

pub fn list_summaries(logs_directory: &Path) -> Vec<RunSummary> {
    let Ok(entries) = fs::read_dir(logs_directory) else {
        return Vec::new();
    };

    let mut summaries: Vec<RunSummary> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == SUMMARY_EXTENSION))
        .filter_map(|path| read_json::<RunSummary>(&path).ok().flatten())
        .collect();

    summaries.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    summaries
}
