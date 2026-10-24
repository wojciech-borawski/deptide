import { beforeEach, describe, expect, it, vi } from "vitest";
import { effectScope, nextTick, ref } from "vue";

import type { PlanPreview, RunPlan } from "@/api/types";

const backend = vi.hoisted(() => ({ previewPlan: vi.fn() }));

vi.mock("@/api/commands", () => ({ ...backend, reportError: () => undefined }));

import { usePlanPreview } from "@/composables/usePlanPreview";

function plan(label: string): RunPlan {
  return {
    projectNames: ["Shop"],
    packages: [],
    steps: ["audit-report"],
    mode: "per-project",
    concurrency: 1,
    dryRun: false,
    extraInstallArgs: [],
    label,
    saveAs: null,
    version: { bump: "patch", when: "always" },
    projectBumps: {},
  };
}

function preview(name: string): PlanPreview {
  return {
    jobs: [
      {
        name,
        dependsOn: [],
        steps: [{ step: "audit-report", commands: ["npm audit --json"], notes: [] }],
        version: null,
      },
    ],
    missing: [],
    withoutPackages: [],
    nothingToDo: null,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (cause: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}

function start(root = "C:\\ws", initial = plan("first")) {
  const scope = effectScope();
  const planRef = ref(initial);
  const state = scope.run(() => usePlanPreview(ref(root), planRef))!;
  return { scope, planRef, state };
}

beforeEach(() => {
  backend.previewPlan.mockReset();
});

describe("plan preview", () => {
  it("loads the preview for the workspace and plan right away", async () => {
    const answer = deferred<PlanPreview>();
    backend.previewPlan.mockReturnValue(answer.promise);
    const { scope, state } = start();

    expect(backend.previewPlan).toHaveBeenCalledWith("C:\\ws", plan("first"));
    expect(state.loading.value).toBe(true);
    answer.resolve(preview("Shop"));
    await vi.waitFor(() => expect(state.loading.value).toBe(false));
    expect(state.preview.value).toEqual(preview("Shop"));
    expect(state.error.value).toBe("");
    scope.stop();
  });

  it("reloads when the plan changes and keeps only the newest answer", async () => {
    const first = deferred<PlanPreview>();
    const second = deferred<PlanPreview>();
    backend.previewPlan.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    const { scope, planRef, state } = start();

    planRef.value.steps.push("build");
    await nextTick();
    expect(backend.previewPlan).toHaveBeenCalledTimes(2);

    second.resolve(preview("newest"));
    await vi.waitFor(() => expect(state.preview.value?.jobs[0]?.name).toBe("newest"));
    first.resolve(preview("stale"));
    await Promise.resolve();
    await nextTick();
    expect(state.preview.value?.jobs[0]?.name).toBe("newest");
    expect(state.loading.value).toBe(false);
    scope.stop();
  });

  it("does not reload for the saved name, or for the run label when nothing is packed", async () => {
    backend.previewPlan.mockResolvedValue(preview("Shop"));
    const { scope, planRef } = start();

    planRef.value.label = "renamed";
    planRef.value.saveAs = "nightly";
    await nextTick();
    expect(backend.previewPlan).toHaveBeenCalledTimes(1);
    scope.stop();
  });

  it("reloads for the run label when a library is packed, since the tarball paths carry it", async () => {
    const labels: string[] = [];
    backend.previewPlan.mockImplementation((_root: string, requested: RunPlan) => {
      labels.push(requested.label);
      return Promise.resolve(preview("Shop"));
    });
    const { scope, planRef } = start("C:\\ws", {
      ...plan("old-label"),
      packages: [{ name: "@demo/core", version: "2.3.0", saveDev: false, savePeer: false, pack: "no-save" }],
    });

    planRef.value.label = "new-label";
    await nextTick();
    planRef.value.saveAs = "nightly";
    await nextTick();
    expect(labels).toEqual(["old-label", "new-label"]);
    scope.stop();
  });

  it("shows the backend error and clears the preview", async () => {
    backend.previewPlan.mockRejectedValue("Cannot read update-libs.json");
    const { scope, state } = start();

    await vi.waitFor(() => expect(state.error.value).toBe("Cannot read update-libs.json"));
    expect(state.preview.value).toBeNull();
    expect(state.loading.value).toBe(false);
    scope.stop();
  });
});
