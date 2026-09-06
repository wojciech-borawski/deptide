mod common;

use common::TempDir;
use deptide_core::domain::{
    Diagnosis, ExecutionMode, InstalledPackage, JobStatus, PackageSpec, RunProjectSummary,
    RunSummary, StepName, StepTiming,
};
use deptide_core::execution::{
    diagnose, render_html, render_markdown, short_integrity, verify_installed,
};

fn lines(items: &[&str]) -> Vec<String> {
    items.iter().map(|line| line.to_string()).collect()
}

#[test]
fn diagnose_recognises_common_npm_failures() {
    let peer = diagnose(&lines(&[
        "npm ERR! code ERESOLVE",
        "npm ERR! unable to resolve dependency tree",
    ]));
    assert_eq!(peer.as_ref().map(|d| d.code.as_str()), Some("ERESOLVE"));
    assert!(peer.unwrap().hint.contains("--legacy-peer-deps"));

    let auth = diagnose(&lines(&[
        "npm ERR! code E401",
        "npm ERR! Unable to authenticate",
    ]));
    assert_eq!(auth.unwrap().code, "E401");

    let missing = diagnose(&lines(&[
        "npm ERR! 404 Not Found - GET https://registry/@lab%2fcore",
    ]));
    assert_eq!(missing.unwrap().code, "E404");

    let build = diagnose(&lines(&[
        "src/main.ts(3,5): error TS2322: Type 'x' is not assignable",
    ]));
    assert_eq!(build.unwrap().title, "TypeScript build errors");

    assert!(diagnose(&lines(&["added 12 packages"])).is_none());
}

#[test]
fn verify_installed_reads_node_modules_and_the_hidden_lockfile() {
    let root = TempDir::new("verify");
    root.write(
        "app/node_modules/@acme/core/package.json",
        r#"{"name":"@acme/core","version":"3.1.0-ABC-1"}"#,
    );
    root.write(
        "app/node_modules/.package-lock.json",
        r#"{"packages":{"node_modules/@acme/core":{"version":"3.1.0-ABC-1","resolved":"https://registry/core.tgz","integrity":"sha512-abcdefghijklmnop"}}}"#,
    );

    let installed = verify_installed(
        &root.join("app"),
        &[
            PackageSpec::new("@acme/core", "3.1.0-ABC-1"),
            PackageSpec::new("@acme/missing", "1.0.0"),
        ],
    );

    assert_eq!(installed.len(), 2);
    assert!(installed[0].matches);
    assert_eq!(installed[0].installed.as_deref(), Some("3.1.0-ABC-1"));
    assert_eq!(
        installed[0].integrity.as_deref(),
        Some("sha512-abcdefghijklmnop")
    );
    assert!(!installed[1].matches);
    assert!(installed[1].installed.is_none());

    assert_eq!(
        short_integrity("sha512-abcdefghijklmnop"),
        "sha512-abcdefghijkl…"
    );

    let ranged = verify_installed(
        &root.join("app"),
        &[PackageSpec::new("@acme/core", "^3.0.0")],
    );
    assert!(
        ranged[0].matches,
        "a range is satisfied by whatever is installed"
    );
    let wrong = verify_installed(
        &root.join("app"),
        &[PackageSpec::new("@acme/core", "3.1.0-ABC-2")],
    );
    assert!(!wrong[0].matches);
}

fn sample_summary() -> RunSummary {
    RunSummary {
        label: "core update".to_string(),
        packages: vec!["@acme/core@3.1.0-ABC-1".to_string()],
        project_count: 2,
        concurrency: 3,
        steps: vec![StepName::Uninstall, StepName::Install],
        mode: ExecutionMode::PerProject,
        dry_run: false,
        started_at: "2026-09-06T10:00:00.000Z".to_string(),
        finished_at: "2026-09-06T10:04:12.000Z".to_string(),
        total_duration_ms: 252_000,
        busy_duration_ms: 400_000,
        log_file: Some("C:/logs/run.log".to_string()),
        projects: vec![
            RunProjectSummary {
                name: "web".to_string(),
                directory: "C:/repos/web".to_string(),
                status: JobStatus::Ok,
                duration_ms: 200_000,
                step_timings: vec![StepTiming {
                    step: StepName::Install,
                    duration_ms: 150_000,
                }],
                error: String::new(),
                warning: String::new(),
                installed: vec![InstalledPackage {
                    name: "@acme/core".to_string(),
                    expected: "3.1.0-ABC-1".to_string(),
                    installed: Some("3.1.0-ABC-1".to_string()),
                    integrity: Some("sha512-abcdefghijklmnop".to_string()),
                    resolved: None,
                    matches: true,
                }],
                diagnosis: None,
                dependency_changes: vec![],
                retries: 0,
            },
            RunProjectSummary {
                name: "api".to_string(),
                directory: "C:/repos/api".to_string(),
                status: JobStatus::Failed,
                duration_ms: 200_000,
                step_timings: vec![],
                error: "npm install exited with code 1".to_string(),
                warning: String::new(),
                installed: vec![],
                diagnosis: Some(Diagnosis {
                    code: "ERESOLVE".to_string(),
                    title: "Peer dependency conflict".to_string(),
                    hint: "Add --legacy-peer-deps".to_string(),
                }),
                dependency_changes: vec![],
                retries: 0,
            },
        ],
        ok_count: 1,
        warn_count: 0,
        failed_count: 1,
        skipped_count: 0,
        aborted: false,
    }
}

#[test]
fn reports_render_both_formats() {
    let summary = sample_summary();

    let markdown = render_markdown(&summary);
    assert!(markdown.starts_with("# Deptide run: core update"));
    assert!(markdown.contains("**4m 12s**"));
    assert!(markdown.contains("| web | ok |"));
    assert!(markdown.contains("sha512-abcdefghijkl…"));
    assert!(markdown.contains("Peer dependency conflict: Add --legacy-peer-deps"));

    let html = render_html(&summary);
    assert!(html.contains("<title>Deptide run: core update</title>"));
    assert!(html.contains("class=\"failed\""));
    assert!(html.contains("4m 12s"));
}
