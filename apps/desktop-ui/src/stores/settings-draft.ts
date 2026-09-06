import { computed, reactive, ref } from "vue";
import { defineStore } from "pinia";

import type { Settings, StepName, UpdateConfig } from "@/api/types";
import { fullSteps } from "@/api/types";
import { orderedSteps, parseArguments } from "@/lib/wizard-plan";

export const defaultBranchPattern = "^([A-Za-z]+-\\d+)";

function splitList(text: string, separator: string): string[] {
  return text
    .split(separator)
    .map((entry) => entry.trim())
    .filter(Boolean);
}

export const useSettingsDraftStore = defineStore("settings-draft", () => {
  const form = reactive<Settings>({
    projectsRoot: "",
    scanDepth: 6,
    concurrency: 3,
    steps: [...fullSteps],
    mode: "per-project",
    extraIgnoredDirectories: [],
    branchSuffixPattern: defaultBranchPattern,
  });
  const ignoredText = ref("");
  const installArgsText = ref("");
  const auditFixArgsText = ref("");
  const transferIgnoreText = ref("");

  const rootMissing = computed(() => !form.projectsRoot.trim());

  function load(settings: Settings, config: UpdateConfig): void {
    Object.assign(form, { ...settings, steps: [...settings.steps] });
    ignoredText.value = settings.extraIgnoredDirectories.join(", ");
    installArgsText.value = (config.installArgs ?? []).join(" ");
    auditFixArgsText.value = (config.auditFixArgs ?? []).join(" ");
    transferIgnoreText.value = (config.transferIgnore ?? []).join("\n");
  }

  function hasStep(step: StepName): boolean {
    return form.steps.includes(step);
  }

  function toggleStep(step: StepName): void {
    const next = hasStep(step) ? form.steps.filter((entry) => entry !== step) : [...form.steps, step];
    form.steps = orderedSteps(next);
  }

  function toSettings(): Settings {
    return {
      ...form,
      steps: form.steps.length ? [...form.steps] : [...fullSteps],
      extraIgnoredDirectories: splitList(ignoredText.value, ","),
    };
  }

  function toConfig(config: UpdateConfig): UpdateConfig {
    return {
      ...config,
      installArgs: parseArguments(installArgsText.value),
      auditFixArgs: parseArguments(auditFixArgsText.value),
      transferIgnore: splitList(transferIgnoreText.value, "\n"),
    };
  }

  return {
    form,
    ignoredText,
    installArgsText,
    auditFixArgsText,
    transferIgnoreText,
    rootMissing,
    load,
    hasStep,
    toggleStep,
    toSettings,
    toConfig,
  };
});
