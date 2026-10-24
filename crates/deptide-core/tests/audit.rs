use deptide_core::domain::AuditCounts;
use deptide_core::execution::parse_audit_report;

#[test]
fn an_audit_without_findings_counts_zero_everywhere() {
    let counts = parse_audit_report(include_str!("fixtures/audit/npm10-clean.json"));

    assert_eq!(counts, Ok(AuditCounts::default()));
}

#[test]
fn an_audit_with_findings_counts_them_per_severity() {
    let counts = parse_audit_report(include_str!("fixtures/audit/npm10-vulnerable.json"));

    assert_eq!(
        counts,
        Ok(AuditCounts {
            info: 0,
            low: 0,
            moderate: 0,
            high: 1,
            critical: 1,
            total: 2,
        })
    );
}

#[test]
fn an_audit_error_names_the_npm_code_and_summary() {
    let error = parse_audit_report(include_str!("fixtures/audit/npm10-no-lockfile.json"));

    assert_eq!(
        error,
        Err("npm audit failed: ENOLOCK This command requires an existing lockfile.".to_string())
    );
}

#[test]
fn an_unreachable_registry_is_an_error_with_npm_message() {
    let error = parse_audit_report(include_str!("fixtures/audit/npm10-unreachable.json"));

    assert_eq!(
        error,
        Err("npm audit failed: request to http://127.0.0.1:9/-/npm/v1/security/audits/quick failed, reason: connect ECONNREFUSED 127.0.0.1:9".to_string())
    );
}

#[test]
fn output_that_is_not_an_audit_report_is_an_error() {
    assert_eq!(
        parse_audit_report("npm error code E404"),
        Err("npm audit did not print a JSON report".to_string())
    );
    assert_eq!(
        parse_audit_report(r#"{"auditReportVersion":2}"#),
        Err("npm audit did not print a JSON report".to_string())
    );
}
