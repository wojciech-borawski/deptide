use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;

use super::tray;
use deptide_core::domain::RunSnapshot;
use deptide_core::execution::{ProgressSink, RunEvent};
use deptide_core::util::time::format_duration;

pub const RUN_EVENT: &str = "run-event";

pub struct TauriSink {
    app: AppHandle,
}

impl TauriSink {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }

    fn notify_finished(&self, snapshot: &RunSnapshot) {
        let Some(summary) = &snapshot.summary else {
            return;
        };

        let title = if summary.aborted {
            format!("Deptide stopped: {}", summary.label)
        } else if summary.failed_count > 0 {
            format!("Deptide finished with failures: {}", summary.label)
        } else {
            format!("Deptide finished: {}", summary.label)
        };

        let body = format!(
            "{} ok, {} warnings, {} failed, {} skipped in {}",
            summary.ok_count,
            summary.warn_count,
            summary.failed_count,
            summary.skipped_count,
            format_duration(summary.total_duration_ms)
        );

        let _ = self
            .app
            .notification()
            .builder()
            .title(title)
            .body(body)
            .show();
    }
}

impl ProgressSink for TauriSink {
    fn emit(&self, event: RunEvent) {
        if let RunEvent::Finished { snapshot, .. } = &event {
            tray::set_idle(&self.app);
            self.notify_finished(snapshot);
        }

        let _ = self.app.emit(RUN_EVENT, event);
    }
}
