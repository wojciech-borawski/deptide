use deptide_core::domain::RunSummary;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCode {
    Success = 0,
    Failures = 1,
    Stopped = 2,
    Usage = 3,
}

impl ExitCode {
    pub fn from_summary(summary: &RunSummary) -> Self {
        if summary.aborted {
            Self::Stopped
        } else if summary.failed_count > 0 {
            Self::Failures
        } else {
            Self::Success
        }
    }
}
