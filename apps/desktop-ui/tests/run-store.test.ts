import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

import type { RunEvent, RunPlan } from "@/api/types";
import { finishedEvent, makeJob, makeOutcome, makeSnapshot } from "./helpers/run-fixtures";

const backend = vi.hoisted(() => ({
  handlers: [] as Array<(event: RunEvent) => void>,
  startRun: vi.fn(),
  abortRun: vi.fn(),
  abortJob: vi.fn(),
  restoreProject: vi.fn(),
  openWorkspace: vi.fn(),
}));

vi.mock("@/api/events", () => ({
  listenRunEvents: async (handler: (event: RunEvent) => void) => {
    backend.handlers.push(handler);
    return () => undefined;
  },
}));

vi.mock("@/api/commands", () => ({
  startRun: backend.startRun,
  abortRun: backend.abortRun,
  abortJob: backend.abortJob,
  restoreProject: backend.restoreProject,
  openWorkspace: backend.openWorkspace,
  reportError: () => undefined,
}));

import { useRunStore } from "@/stores/run";

const plan: RunPlan = {
  projectNames: ["web", "api"],
  packages: [{ name: "left-pad", version: "1.3.0", saveDev: false }],
  steps: ["install"],
  mode: "per-project",
  concurrency: 2,
  dryRun: true,
  extraInstallArgs: [],
  label: "test",
  saveAs: null,
  version: { bump: "patch", onlyIfSameAsMain: false },
};

function emit(event: RunEvent): void {
  for (const handler of backend.handlers) handler(event);
}

describe("run store", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    backend.handlers.length = 0;
    vi.clearAllMocks();
    vi.useFakeTimers();
    vi.setSystemTime(1_000);
  });

  it("starts a run, keeps the initial snapshot and replays events that arrived during the start call", async () => {
    const store = useRunStore();
    await store.subscribe();
    const snapshot = makeSnapshot("run-1", [makeJob("web"), makeJob("api")]);

    let resolveStart: (value: ReturnType<typeof makeOutcome>) => void = () => undefined;
    backend.startRun.mockReturnValue(new Promise((resolve) => (resolveStart = resolve)));

    const starting = store.start("C:\\workspace", plan);
    emit({ type: "logLine", runId: "run-1", job: "web", line: "$ npm install" });
    emit({ type: "jobChanged", runId: "run-1", job: makeJob("web", { status: "running" }) });
    resolveStart(makeOutcome(snapshot));
    await starting;

    expect(store.isActive).toBe(true);
    expect(store.selectedJob).toBe("web");
    expect(store.logLinesFor("web")).toEqual(["$ npm install"]);
    expect(store.jobs[0]?.status).toBe("running");
    expect(store.statusCounts.running).toBe(1);
    expect(store.statusCounts.pending).toBe(1);
  });

  it("ignores events from other runs and caps the log per project", async () => {
    const store = useRunStore();
    await store.subscribe();
    backend.startRun.mockResolvedValue(makeOutcome(makeSnapshot("run-1", [makeJob("web")])));
    await store.start("C:\\workspace", plan);

    emit({ type: "logLine", runId: "other", job: "web", line: "not mine" });
    expect(store.logLinesFor("web")).toEqual([]);

    for (let index = 0; index < 2_100; index += 1) {
      emit({ type: "logLine", runId: "run-1", job: "web", line: `line ${index}` });
    }
    expect(store.logLinesFor("web")).toHaveLength(2_000);
    expect(store.logLinesFor("web").at(-1)).toBe("line 2099");
    expect(store.logLinesFor("web")[0]).toBe("line 100");
  });

  it("stops the clock and takes the summary when the run finishes", async () => {
    const store = useRunStore();
    await store.subscribe();
    const snapshot = makeSnapshot("run-1", [makeJob("web")]);
    backend.startRun.mockResolvedValue(makeOutcome(snapshot));
    await store.start("C:\\workspace", plan);

    vi.advanceTimersByTime(1_200);
    expect(store.elapsedMs).toBeGreaterThanOrEqual(1_000);

    emit(finishedEvent({ ...snapshot, jobs: [makeJob("web", { status: "ok" })] }, { totalDurationMs: 5_000 }));

    expect(store.isActive).toBe(false);
    expect(store.elapsedMs).toBe(5_000);
    expect(store.summary?.okCount).toBe(1);
    expect(store.finishedCount).toBe(1);
  });

  it("reports a failed start as an error instead of throwing", async () => {
    const store = useRunStore();
    await store.subscribe();
    backend.startRun.mockRejectedValue(new Error("A run is already in progress"));

    const outcome = await store.start("C:\\workspace", plan);

    expect(outcome).toBeNull();
    expect(store.error).toContain("already in progress");
    expect(store.starting).toBe(false);
  });

  it("forwards abort requests with the current run id", async () => {
    const store = useRunStore();
    await store.subscribe();
    backend.startRun.mockResolvedValue(makeOutcome(makeSnapshot("run-7", [makeJob("web")])));
    backend.abortRun.mockResolvedValue(undefined);
    backend.abortJob.mockResolvedValue(undefined);
    await store.start("C:\\workspace", plan);

    await store.abort();
    await store.abortJob("web");

    expect(backend.abortRun).toHaveBeenCalledWith("run-7");
    expect(backend.abortJob).toHaveBeenCalledWith("run-7", "web");
  });
});
