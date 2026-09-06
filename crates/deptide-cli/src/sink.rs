use std::collections::HashMap;
use std::io::{self, Write};
use std::sync::Mutex;

use deptide_core::domain::{JobSnapshot, JobStatus};
use deptide_core::execution::{ProgressSink, RunEvent};
use deptide_core::util::time::format_duration;

pub struct TerminalSink {
    show_output: bool,
    last_status: Mutex<HashMap<String, JobStatus>>,
}

impl TerminalSink {
    pub fn new(show_output: bool) -> Self {
        Self {
            show_output,
            last_status: Mutex::new(HashMap::new()),
        }
    }

    fn print(&self, line: String) {
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        let _ = writeln!(handle, "{line}");
    }

    fn status_changed(&self, job: &JobSnapshot) -> bool {
        let mut seen = self
            .last_status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        seen.insert(job.name.clone(), job.status) != Some(job.status)
    }

    fn describe(job: &JobSnapshot) -> String {
        let name = &job.name;
        match job.status {
            JobStatus::Pending => format!("{name}: pending"),
            JobStatus::Running => format!("{name}: running"),
            JobStatus::Ok => format!("{name}: ok in {}", format_duration(job.duration_ms)),
            JobStatus::Warn => format!(
                "{name}: ok with warning in {} ({})",
                format_duration(job.duration_ms),
                job.warning
            ),
            JobStatus::Failed => format!("{name}: failed ({})", job.error),
            JobStatus::Skipped => format!("{name}: skipped ({})", job.error),
        }
    }
}

impl ProgressSink for TerminalSink {
    fn emit(&self, event: RunEvent) {
        match event {
            RunEvent::JobChanged { job, .. } => {
                if self.status_changed(&job) {
                    self.print(Self::describe(&job));
                }
            }
            RunEvent::LogLine { job, line, .. } => {
                if self.show_output {
                    self.print(format!("  [{job}] {line}"));
                }
            }
            RunEvent::PhaseChanged {
                phase: Some(step), ..
            } => self.print(format!("-- step: {}", step.label())),
            RunEvent::PhaseChanged { phase: None, .. } | RunEvent::Finished { .. } => {}
        }
    }
}
