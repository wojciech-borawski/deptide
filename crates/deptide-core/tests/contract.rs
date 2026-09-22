use deptide_core::domain::{
    Diagnosis, ExecutionMode, InstalledPackage, Job, JobSnapshot, JobStatus, PackageSpec, RunPlan,
    StepName, StepTiming, VersionPolicy,
};
use deptide_core::execution::{JobState, RunEvent, RunOptions, RunState};

fn keys(value: &serde_json::Value) -> Vec<String> {
    value
        .as_object()
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default()
}

#[test]
fn job_snapshot_uses_the_camel_case_names_the_frontend_expects() {
    let mut state = JobState::new(Job {
        name: "web".to_string(),
        directory: "C:/repos/web".into(),
        packages: vec![PackageSpec::new("@acme/core", "1.0.0")],
        steps: vec![StepName::ForceInstall],
        install_args: vec![],
        audit_fix_args: vec![],
        depends_on: vec!["core".to_string()],
        command: None,
        version: VersionPolicy::default(),
    });
    state.record_step(StepName::ForceInstall, 5);
    state.installed.push(InstalledPackage {
        name: "@acme/core".to_string(),
        expected: "1.0.0".to_string(),
        installed: Some("1.0.0".to_string()),
        integrity: None,
        resolved: None,
        matches: true,
    });
    state.diagnosis = Some(Diagnosis {
        code: "X".to_string(),
        title: "t".to_string(),
        hint: "h".to_string(),
    });

    let json = serde_json::to_value(state.snapshot(true)).unwrap();
    let names = keys(&json);

    for expected in [
        "name",
        "directory",
        "packages",
        "status",
        "currentStep",
        "startedAtMs",
        "stepStartedAtMs",
        "durationMs",
        "stepTimings",
        "error",
        "warning",
        "dependsOn",
        "installed",
        "diagnosis",
        "dependencyChanges",
        "retries",
        "backupAvailable",
        "log",
    ] {
        assert!(
            names.contains(&expected.to_string()),
            "missing {expected} in {names:?}"
        );
    }

    assert_eq!(json["status"], "pending");
    assert_eq!(json["stepTimings"][0]["step"], "force-install");
    assert_eq!(json["installed"][0]["matches"], true);
}

#[test]
fn run_events_are_tagged_for_the_frontend_switch() {
    let snapshot = JobSnapshot {
        name: "web".to_string(),
        directory: String::new(),
        packages: vec![],
        status: JobStatus::Running,
        current_step: String::new(),
        started_at_ms: None,
        step_started_at_ms: None,
        duration_ms: 0,
        step_timings: vec![StepTiming {
            step: StepName::Build,
            duration_ms: 1,
        }],
        error: String::new(),
        warning: String::new(),
        depends_on: vec![],
        installed: vec![],
        diagnosis: None,
        dependency_changes: vec![],
        retries: 0,
        backup_available: false,
        log: vec![],
    };

    let job = serde_json::to_value(RunEvent::JobChanged {
        run_id: "r".to_string(),
        job: snapshot,
    })
    .unwrap();
    assert_eq!(job["type"], "jobChanged");
    assert_eq!(job["runId"], "r");
    assert_eq!(job["job"]["status"], "running");

    let line = serde_json::to_value(RunEvent::LogLine {
        run_id: "r".to_string(),
        job: "web".to_string(),
        line: "x".to_string(),
    })
    .unwrap();
    assert_eq!(line["type"], "logLine");

    let phase = serde_json::to_value(RunEvent::PhaseChanged {
        run_id: "r".to_string(),
        phase: Some(StepName::Audit),
    })
    .unwrap();
    assert_eq!(phase["type"], "phaseChanged");
    assert_eq!(phase["phase"], "audit");
}

#[test]
fn run_plan_and_snapshot_round_trip_through_json() {
    let plan: RunPlan = serde_json::from_str(
        r#"{"projectNames":["web"],"packages":[{"name":"@acme/core","version":"1.0.0","saveDev":false}],
            "steps":["uninstall","force-install"],"mode":"per-step","concurrency":2,"dryRun":true,
            "extraInstallArgs":["--force"],"label":"x","saveAs":null}"#,
    )
    .unwrap();
    assert_eq!(plan.mode, ExecutionMode::PerStep);
    assert_eq!(
        plan.steps,
        vec![StepName::Uninstall, StepName::ForceInstall]
    );

    let state = RunState::new(
        "run".to_string(),
        "x".to_string(),
        vec![],
        &RunOptions {
            concurrency: 2,
            mode: ExecutionMode::PerStep,
            dry_run: true,
        },
    );
    let json = serde_json::to_value(state.snapshot(false)).unwrap();
    for expected in [
        "id",
        "label",
        "packages",
        "steps",
        "mode",
        "concurrency",
        "dryRun",
        "startedAtMs",
        "finishedAtMs",
        "currentPhase",
        "aborted",
        "jobs",
        "summary",
    ] {
        assert!(
            keys(&json).contains(&expected.to_string()),
            "missing {expected}"
        );
    }
    assert_eq!(json["mode"], "per-step");
}
