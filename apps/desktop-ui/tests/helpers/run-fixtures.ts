import type { JobSnapshot, RunEvent, RunSnapshot, RunStartOutcome, RunSummary } from "@/api/types";

export function makeJob(name: string, overrides: Partial<JobSnapshot> = {}): JobSnapshot {
  return {
    name,
    directory: `C:\\repos\\${name}`,
    packages: ["left-pad@1.3.0"],
    status: "pending",
    currentStep: "",
    startedAtMs: null,
    stepStartedAtMs: null,
    durationMs: 0,
    stepTimings: [],
    error: "",
    warning: "",
    dependsOn: [],
    installed: [],
    diagnosis: null,
    dependencyChanges: [],
    retries: 0,
    backupAvailable: false,
    log: [],
    ...overrides,
  };
}

export function makeSnapshot(id: string, jobs: JobSnapshot[], overrides: Partial<RunSnapshot> = {}): RunSnapshot {
  return {
    id,
    label: "test",
    packages: ["left-pad@1.3.0"],
    steps: ["install"],
    mode: "per-project",
    concurrency: 2,
    dryRun: true,
    startedAtMs: 1_000,
    finishedAtMs: null,
    currentPhase: null,
    aborted: false,
    command: null,
    jobs,
    summary: null,
    ...overrides,
  };
}

export function makeSummary(snapshot: RunSnapshot, overrides: Partial<RunSummary> = {}): RunSummary {
  return {
    label: snapshot.label,
    packages: snapshot.packages,
    projectCount: snapshot.jobs.length,
    concurrency: snapshot.concurrency,
    steps: snapshot.steps,
    mode: snapshot.mode,
    dryRun: snapshot.dryRun,
    startedAt: new Date(snapshot.startedAtMs).toISOString(),
    finishedAt: new Date(snapshot.startedAtMs + 5_000).toISOString(),
    totalDurationMs: 5_000,
    busyDurationMs: 4_000,
    logFile: null,
    projects: [],
    okCount: snapshot.jobs.length,
    warnCount: 0,
    failedCount: 0,
    skippedCount: 0,
    aborted: false,
    ...overrides,
  };
}

export function makeOutcome(snapshot: RunSnapshot): RunStartOutcome {
  return { runId: snapshot.id, snapshot, missing: [], withoutPackages: [], savedRunPath: null };
}

export function finishedEvent(snapshot: RunSnapshot, summary: Partial<RunSummary> = {}): RunEvent {
  const finished: RunSnapshot = {
    ...snapshot,
    finishedAtMs: snapshot.startedAtMs + 5_000,
    summary: makeSummary(snapshot, summary),
  };
  return { type: "finished", runId: snapshot.id, snapshot: finished };
}
