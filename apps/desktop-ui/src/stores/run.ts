import { ref } from "vue";
import { defineStore } from "pinia";

import * as api from "@/api/commands";
import type { RunPlan, RunStartOutcome, StepName } from "@/api/types";
import { useRunTracker } from "@/composables/useRunTracker";
import { buildRerunPlan } from "@/lib/wizard-plan";
import { useWorkspaceStore } from "./workspace";

export const useRunStore = defineStore("run", () => {
  const tracker = useRunTracker({ onFinished: () => void useWorkspaceStore().refresh() });
  const lastStartOutcome = ref<RunStartOutcome | null>(null);
  const lastPlan = ref<RunPlan | null>(null);

  async function start(root: string, plan: RunPlan): Promise<RunStartOutcome | null> {
    const outcome = await tracker.track(() => api.startRun(root, plan));
    if (outcome) {
      lastStartOutcome.value = outcome;
      lastPlan.value = plan;
    }
    return outcome;
  }

  async function rerun(root: string, projectNames: string[], steps: StepName[]): Promise<RunStartOutcome | null> {
    if (!lastPlan.value) return null;
    return start(root, buildRerunPlan(lastPlan.value, projectNames, steps));
  }

  async function restoreProject(name: string): Promise<string[]> {
    if (!tracker.snapshot.value) return [];
    return api.restoreProject(tracker.snapshot.value.id, name);
  }

  return {
    ...tracker,
    lastStartOutcome,
    lastPlan,
    start,
    rerun,
    restoreProject,
  };
});
