use serde::Deserialize;

use crate::domain::AuditCounts;

const NOT_A_REPORT: &str = "npm audit did not print a JSON report";

#[derive(Deserialize)]
#[serde(untagged)]
enum AuditOutput {
    Report { metadata: AuditMetadata },
    Failure(AuditFailure),
}

#[derive(Deserialize)]
struct AuditMetadata {
    vulnerabilities: AuditCounts,
}

#[derive(Deserialize)]
struct AuditFailure {
    #[serde(default)]
    message: String,
    error: AuditError,
}

#[derive(Deserialize)]
struct AuditError {
    #[serde(default)]
    code: String,
    #[serde(default)]
    summary: String,
}

impl AuditFailure {
    fn describe(&self) -> String {
        let parts: Vec<&str> = [
            self.error.code.as_str(),
            self.error.summary.as_str(),
            self.message.as_str(),
        ]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect();
        format!("npm audit failed: {}", parts.join(" "))
    }
}

/// The vulnerability counts of an `npm audit --json` report, or why the output holds none.
pub fn parse_audit_report(stdout: &str) -> Result<AuditCounts, String> {
    match serde_json::from_str::<AuditOutput>(stdout) {
        Ok(AuditOutput::Report { metadata }) => Ok(metadata.vulnerabilities),
        Ok(AuditOutput::Failure(failure)) => Err(failure.describe()),
        Err(_) => Err(NOT_A_REPORT.to_string()),
    }
}

pub fn describe_audit(counts: &AuditCounts) -> String {
    format!(
        "audit: {} vulnerabilities ({} critical, {} high, {} moderate, {} low, {} info)",
        counts.total, counts.critical, counts.high, counts.moderate, counts.low, counts.info
    )
}

/// The counts of every project that ran the audit report, added up; `None` when none did.
pub fn audit_totals<'a>(audits: impl IntoIterator<Item = &'a AuditCounts>) -> Option<AuditCounts> {
    audits.into_iter().fold(None, |sum, counts| {
        let sum = sum.unwrap_or_default();
        Some(AuditCounts {
            info: sum.info + counts.info,
            low: sum.low + counts.low,
            moderate: sum.moderate + counts.moderate,
            high: sum.high + counts.high,
            critical: sum.critical + counts.critical,
            total: sum.total + counts.total,
        })
    })
}

/// The counts from the most to the least severe, the order reports list them in.
pub fn audit_columns(counts: &AuditCounts) -> [u32; 6] {
    [
        counts.critical,
        counts.high,
        counts.moderate,
        counts.low,
        counts.info,
        counts.total,
    ]
}
