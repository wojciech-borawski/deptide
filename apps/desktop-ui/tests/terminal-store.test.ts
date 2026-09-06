import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

import type { RunEvent } from "@/api/types";
import { finishedEvent, makeJob, makeOutcome, makeSnapshot } from "./helpers/run-fixtures";

const backend = vi.hoisted(() => ({
  handlers: [] as Array<(event: RunEvent) => void>,
  startCommandRun: vi.fn(),
  abortRun: vi.fn(),
  abortJob: vi.fn(),
}));

vi.mock("@/api/events", () => ({
  listenRunEvents: async (handler: (event: RunEvent) => void) => {
    backend.handlers.push(handler);
    return () => undefined;
  },
}));

vi.mock("@/api/commands", () => ({
  startCommandRun: backend.startCommandRun,
  abortRun: backend.abortRun,
  abortJob: backend.abortJob,
  reportError: () => undefined,
}));

import { useTerminalStore } from "@/stores/terminal";

function emit(event: RunEvent): void {
  for (const handler of backend.handlers) handler(event);
}

describe("terminal store", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    backend.handlers.length = 0;
    vi.clearAllMocks();
    vi.useFakeTimers();
    vi.setSystemTime(1_000);
  });

  it("only allows a start with projects and a non-empty command", () => {
    const store = useTerminalStore();
    expect(store.canStart).toBe(false);

    store.setProjects(["web"]);
    expect(store.canStart).toBe(false);

    store.command = "   ";
    expect(store.canStart).toBe(false);

    store.command = "git status";
    expect(store.canStart).toBe(true);
  });

  it("toggles projects in and out of the selection", () => {
    const store = useTerminalStore();
    store.setProjects(["web"]);
    store.toggleProject("api");
    store.toggleProject("web");
    expect(store.projectNames).toEqual(["api"]);
  });

  it("sends the trimmed command with the concurrency and remembers it in the history", async () => {
    const store = useTerminalStore();
    await store.subscribe();
    store.setProjects(["web", "api"]);
    store.command = "  npm test ";
    store.concurrency = 0;
    const snapshot = makeSnapshot("cmd-1", [makeJob("web"), makeJob("api")], { command: "npm test" });
    backend.startCommandRun.mockResolvedValue(makeOutcome(snapshot));

    expect(await store.start("C:\\workspace")).toBe(true);

    expect(backend.startCommandRun).toHaveBeenCalledWith("C:\\workspace", ["web", "api"], "npm test", 1);
    expect(store.history[0]).toBe("npm test");
    expect(store.isActive).toBe(true);
    expect(store.canStart).toBe(false);
  });

  it("tracks output per project and finishes with the summary", async () => {
    const store = useTerminalStore();
    await store.subscribe();
    store.setProjects(["web"]);
    store.command = "git status";
    const snapshot = makeSnapshot("cmd-2", [makeJob("web")], { command: "git status" });
    backend.startCommandRun.mockResolvedValue(makeOutcome(snapshot));
    await store.start("C:\\workspace");

    emit({ type: "logLine", runId: "cmd-2", job: "web", line: "On branch main" });
    emit({
      type: "jobChanged",
      runId: "cmd-2",
      job: makeJob("web", { status: "failed", error: "command exited with code 1" }),
    });
    emit(finishedEvent({ ...snapshot, jobs: [makeJob("web", { status: "failed" })] }, { failedCount: 1, okCount: 0 }));

    expect(store.logLinesFor("web")).toEqual(["On branch main"]);
    expect(store.statusCounts.failed).toBe(1);
    expect(store.summary?.failedCount).toBe(1);
    expect(store.isActive).toBe(false);
    expect(store.elapsedMs).toBe(5_000);
  });
});
