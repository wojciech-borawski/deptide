import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

import type { DependencyDrift, DependencyGraph, ProjectCopies } from "@/api/types";

vi.mock("@/api/commands", () => ({
  readDependencyDrift: vi.fn(),
  readDependencyGraph: vi.fn(),
  findInstalledCopies: vi.fn(),
  openDependencyRequest: vi.fn(),
  cancelDependencyRequest: vi.fn(),
}));

import * as api from "@/api/commands";
import { useDependenciesStore } from "@/stores/dependencies";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (cause: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}

function drift(project: string): DependencyDrift {
  return { projects: [project], libraries: [], unreadable: [] };
}

function graph(project: string): DependencyGraph {
  return { nodes: [{ name: project, packageName: null, layer: 0, dependsOn: [] }], edges: [], cycles: [] };
}

function copies(project: string): ProjectCopies[] {
  return [{ project, hasNodeModules: true, packages: [] }];
}

function settled(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

function lastRequestId(mock: { mock: { calls: unknown[][] } }): number {
  return mock.mock.calls.at(-1)?.at(-1) as number;
}

describe("dependencies store requests", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
    let issued = 0;
    vi.mocked(api.openDependencyRequest).mockImplementation(async () => ++issued);
  });

  it("drops results, errors and busy state of requests started for the previous workspace", async () => {
    const versions = deferred<DependencyDrift>();
    const layers = deferred<DependencyGraph>();
    const duplicates = deferred<ProjectCopies[]>();
    vi.mocked(api.readDependencyDrift).mockReturnValue(versions.promise);
    vi.mocked(api.readDependencyGraph).mockReturnValue(layers.promise);
    vi.mocked(api.findInstalledCopies).mockReturnValue(duplicates.promise);
    const store = useDependenciesStore();
    store.openRoot("A");
    const pending = [store.loadDrift(true), store.loadGraph(), store.scanCopies()];
    await settled();

    store.openRoot("B");
    const busyAfterSwitch = [store.driftBusy, store.graphBusy, store.copiesBusy, store.checkingLatest];
    versions.resolve(drift("A-project"));
    layers.resolve(graph("A-project"));
    duplicates.reject("A failed");
    await Promise.all(pending);

    expect(busyAfterSwitch).toEqual([false, false, false, false]);
    expect([store.drift, store.graph, store.copies]).toEqual([null, null, null]);
    expect(store.copiesError).toBe("");
  });

  it("keeps the newest request's result whichever order the responses arrive in", async () => {
    const outcomes: string[] = [];
    for (const order of [
      [0, 1],
      [1, 0],
    ]) {
      setActivePinia(createPinia());
      const responses = [deferred<DependencyDrift>(), deferred<DependencyDrift>()];
      vi.mocked(api.readDependencyDrift)
        .mockReturnValueOnce(responses[0]!.promise)
        .mockReturnValueOnce(responses[1]!.promise);
      const store = useDependenciesStore();
      store.openRoot("A");
      const loads = [store.loadDrift(false)];
      await settled();
      loads.push(store.loadDrift(true));
      await settled();
      for (const index of order) {
        responses[index]!.resolve(drift(`answer-${index}`));
        await loads[index];
      }
      outcomes.push(`${store.drift?.projects[0]} busy=${store.driftBusy}`);
    }

    expect(outcomes).toEqual(["answer-1 busy=false", "answer-1 busy=false"]);
  });

  it("busy follows only the current request", async () => {
    const older = deferred<DependencyDrift>();
    const newer = deferred<DependencyDrift>();
    vi.mocked(api.readDependencyDrift).mockReturnValueOnce(older.promise).mockReturnValueOnce(newer.promise);
    const store = useDependenciesStore();
    store.openRoot("A");
    const first = store.loadDrift(false);
    await settled();
    const second = store.loadDrift(true);
    await settled();

    older.resolve(drift("older"));
    await first;
    const busyWhileNewerRuns = [store.driftBusy, store.checkingLatest];
    newer.resolve(drift("newer"));
    await second;

    expect(busyWhileNewerRuns).toEqual([true, true]);
    expect([store.driftBusy, store.checkingLatest]).toEqual([false, false]);
  });

  it("cancels superseded drift and duplicate requests in the backend by request id", async () => {
    vi.mocked(api.readDependencyDrift).mockReturnValue(new Promise(() => {}));
    vi.mocked(api.findInstalledCopies).mockReturnValue(new Promise(() => {}));
    const store = useDependenciesStore();
    store.openRoot("A");

    void store.loadDrift(true);
    await settled();
    const firstDrift = lastRequestId(vi.mocked(api.readDependencyDrift));
    void store.loadDrift(true);
    await settled();
    const secondDrift = lastRequestId(vi.mocked(api.readDependencyDrift));
    void store.scanCopies();
    await settled();
    const scan = lastRequestId(vi.mocked(api.findInstalledCopies));
    const cancelledBeforeSwitch = vi.mocked(api.cancelDependencyRequest).mock.calls.map(([id]) => id);
    store.openRoot("B");

    expect(new Set([firstDrift, secondDrift, scan]).size).toBe(3);
    expect(cancelledBeforeSwitch).toEqual([firstDrift]);
    expect(vi.mocked(api.cancelDependencyRequest).mock.calls.map(([id]) => id)).toEqual([
      firstDrift,
      secondDrift,
      scan,
    ]);
  });

  it("cancelAll stops the running requests when the screen is left and ignores what they return", async () => {
    const versions = deferred<DependencyDrift>();
    const duplicates = deferred<ProjectCopies[]>();
    vi.mocked(api.readDependencyDrift).mockReturnValue(versions.promise);
    vi.mocked(api.findInstalledCopies).mockReturnValue(duplicates.promise);
    const store = useDependenciesStore();
    store.openRoot("A");
    const pending = [store.loadDrift(true), store.scanCopies()];
    await settled();
    const ids = [lastRequestId(vi.mocked(api.readDependencyDrift)), lastRequestId(vi.mocked(api.findInstalledCopies))];

    store.cancelAll();
    versions.resolve(drift("late"));
    duplicates.resolve(copies("late"));
    await Promise.all(pending);

    expect(vi.mocked(api.cancelDependencyRequest).mock.calls.map(([id]) => id)).toEqual(ids);
    expect([store.drift, store.copies, store.driftBusy, store.copiesBusy]).toEqual([null, null, false, false]);
  });

  it("still loads the current workspace after a switch", async () => {
    vi.mocked(api.readDependencyDrift).mockResolvedValue(drift("B-project"));
    const store = useDependenciesStore();
    store.openRoot("A");
    store.openRoot("B");

    await store.loadDrift(false);

    expect(vi.mocked(api.readDependencyDrift).mock.calls[0]?.slice(0, 2)).toEqual(["B", false]);
    expect(store.drift?.projects).toEqual(["B-project"]);
  });

  it("runs drift and duplicate requests under the ids the backend issued", async () => {
    vi.mocked(api.openDependencyRequest).mockResolvedValueOnce(41).mockResolvedValueOnce(42);
    vi.mocked(api.readDependencyDrift).mockResolvedValue(drift("A-project"));
    vi.mocked(api.findInstalledCopies).mockResolvedValue(copies("A-project"));
    const store = useDependenciesStore();
    store.openRoot("A");

    await store.loadDrift(false);
    await store.scanCopies();

    expect(lastRequestId(vi.mocked(api.readDependencyDrift))).toBe(41);
    expect(lastRequestId(vi.mocked(api.findInstalledCopies))).toBe(42);
    expect(store.drift?.projects).toEqual(["A-project"]);
  });

  it("cancels a request dropped while its id was being issued and never starts its work", async () => {
    const issuing = deferred<number>();
    vi.mocked(api.openDependencyRequest).mockReturnValueOnce(issuing.promise);
    vi.mocked(api.readDependencyDrift).mockResolvedValue(drift("late"));
    const store = useDependenciesStore();
    store.openRoot("A");

    const pending = store.loadDrift(true);
    store.cancelAll();
    issuing.resolve(7);
    await pending;

    expect(vi.mocked(api.cancelDependencyRequest).mock.calls).toEqual([[7]]);
    expect(api.readDependencyDrift).not.toHaveBeenCalled();
    expect([store.drift, store.driftBusy]).toEqual([null, false]);
  });
});
