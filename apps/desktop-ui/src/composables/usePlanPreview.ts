import { ref, watch, type Ref } from "vue";

import * as api from "@/api/commands";
import type { PlanPreview, RunPlan } from "@/api/types";
import { describeError } from "./useAsyncAction";

export function usePlanPreview(root: Ref<string>, plan: Ref<RunPlan>) {
  const preview = ref<PlanPreview | null>(null);
  const loading = ref(false);
  const error = ref("");
  let latest = 0;

  async function load(): Promise<void> {
    latest += 1;
    const request = latest;
    loading.value = true;
    error.value = "";
    try {
      const answer = await api.previewPlan(root.value, plan.value);
      if (request !== latest) return;
      preview.value = answer;
    } catch (cause) {
      if (request !== latest) return;
      preview.value = null;
      error.value = describeError(cause);
    } finally {
      if (request === latest) loading.value = false;
    }
  }

  const planKey = () => {
    const packs = plan.value.packages.some((spec) => spec.pack);
    return JSON.stringify({ ...plan.value, label: packs ? plan.value.label : "", saveAs: null });
  };

  watch([root, planKey], () => void load(), { immediate: true });

  return { preview, loading, error, reload: load };
}
