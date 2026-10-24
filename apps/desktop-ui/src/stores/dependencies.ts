import { computed, ref } from "vue";
import { defineStore } from "pinia";

import * as api from "@/api/commands";
import type { CopyScope, DependencyDrift, DependencyGraph, ProjectCopies } from "@/api/types";
import { describeError } from "@/composables/useAsyncAction";

export const dependencyTabs = ["versions", "graph", "duplicates"] as const;

export type DependencyTab = (typeof dependencyTabs)[number];

type RequestIds = {
  open: () => Promise<number>;
  cancel: (requestId: number) => void;
};

const backendRequests: RequestIds = {
  open: () => api.openDependencyRequest(),
  cancel: (requestId) => api.cancelDependencyRequest(requestId),
};

const localRequests: RequestIds = {
  open: () => Promise.resolve(0),
  cancel: () => undefined,
};

type RunningRequest = { requestId: number | null };

function useCurrentRequest(ids: RequestIds) {
  const busy = ref(false);
  const error = ref("");
  let current: RunningRequest | null = null;

  function drop(): void {
    if (current?.requestId != null) {
      ids.cancel(current.requestId);
    }
    current = null;
    busy.value = false;
    error.value = "";
  }

  async function run<T>(work: (requestId: number) => Promise<T>, apply: (value: T) => void): Promise<void> {
    drop();
    const running: RunningRequest = { requestId: null };
    current = running;
    busy.value = true;
    try {
      const requestId = await ids.open();
      if (current !== running) {
        ids.cancel(requestId);
        return;
      }
      running.requestId = requestId;
      const value = await work(requestId);
      if (current === running) {
        apply(value);
      }
    } catch (cause) {
      if (current === running) {
        error.value = describeError(cause);
      }
    } finally {
      if (current === running) {
        current = null;
        busy.value = false;
      }
    }
  }

  return { busy, error, run, drop };
}

export const useDependenciesStore = defineStore("dependencies", () => {
  const tab = ref<DependencyTab>("versions");
  const root = ref("");
  const drift = ref<DependencyDrift | null>(null);
  const graph = ref<DependencyGraph | null>(null);
  const copies = ref<ProjectCopies[] | null>(null);
  const copyScope = ref<CopyScope>("libraries");
  const driftChecksLatest = ref(false);
  const driftRequest = useCurrentRequest(backendRequests);
  const graphRequest = useCurrentRequest(localRequests);
  const copiesRequest = useCurrentRequest(backendRequests);
  const checkingLatest = computed(() => driftRequest.busy.value && driftChecksLatest.value);

  function cancelAll(): void {
    driftRequest.drop();
    graphRequest.drop();
    copiesRequest.drop();
  }

  function openRoot(next: string): void {
    if (root.value === next) return;
    cancelAll();
    root.value = next;
    drift.value = null;
    graph.value = null;
    copies.value = null;
  }

  async function loadDrift(checkLatest: boolean): Promise<void> {
    driftChecksLatest.value = checkLatest;
    await driftRequest.run(
      (requestId) => api.readDependencyDrift(root.value, checkLatest, requestId),
      (value) => (drift.value = value),
    );
  }

  async function loadGraph(): Promise<void> {
    await graphRequest.run(
      () => api.readDependencyGraph(root.value),
      (value) => (graph.value = value),
    );
  }

  async function scanCopies(): Promise<void> {
    await copiesRequest.run(
      (requestId) => api.findInstalledCopies(root.value, copyScope.value, requestId),
      (value) => (copies.value = value),
    );
  }

  return {
    tab,
    root,
    drift,
    graph,
    copies,
    copyScope,
    checkingLatest,
    driftBusy: driftRequest.busy,
    driftError: driftRequest.error,
    graphBusy: graphRequest.busy,
    graphError: graphRequest.error,
    copiesBusy: copiesRequest.busy,
    copiesError: copiesRequest.error,
    openRoot,
    loadDrift,
    loadGraph,
    scanCopies,
    cancelAll,
  };
});
