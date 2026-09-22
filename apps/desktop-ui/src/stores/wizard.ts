import { computed, reactive, ref } from "vue";
import { defineStore } from "pinia";

import * as api from "@/api/commands";
import { useAsyncAction } from "@/composables/useAsyncAction";
import type { PackageSpec, ProjectInspection, ProjectView, RunSnapshot, SavedRun, Settings } from "@/api/types";
import {
  buildRunPlan,
  createChoice,
  draftProblems,
  selectedPackages,
  suggestLabel,
  type PackageChoice,
  type WizardDraft,
} from "@/lib/wizard-plan";
import { parseSpec } from "@/lib/versions";

export const wizardSteps = ["projects", "libraries", "options", "review"] as const;

function emptyDraft(): WizardDraft {
  return {
    projectNames: [],
    choices: [],
    manualPackages: [],
    steps: ["uninstall", "install", "audit", "build"],
    mode: "per-project",
    concurrency: 3,
    dryRun: false,
    extraInstallArgs: "",
    versionBump: "patch",
    bumpOnlyIfSameAsMain: false,
    label: "",
    saveRun: true,
    saveName: "",
  };
}

function mergeChoices(previous: PackageChoice[], inspection: ProjectInspection): PackageChoice[] {
  return inspection.candidates.map((candidate) => {
    const fresh = createChoice(candidate);
    const known = previous.find((choice) => choice.name === candidate.name);
    if (!known) return fresh;

    return {
      ...fresh,
      selected: known.selected,
      mode: known.mode,
      suffix: known.suffix,
      manualVersion: known.manualVersion,
      keepRange: known.keepRange || fresh.keepRange,
    };
  });
}

export const useWizardStore = defineStore("wizard", () => {
  const stepIndex = ref(0);
  const draft = reactive<WizardDraft>(emptyDraft());
  const inspection = ref<ProjectInspection | null>(null);
  const inspectAction = useAsyncAction();
  const inspecting = inspectAction.busy;
  const inspectionError = inspectAction.error;
  const inspectedKey = ref("");
  const labelTouched = ref(false);
  const initializedRoot = ref("");

  const problems = computed(() => draftProblems(draft, stepIndex.value));
  const canProceed = computed(() => problems.value.length === 0);
  const packages = computed(() => selectedPackages(draft));
  const plan = computed(() => buildRunPlan(draft));
  const isLastStep = computed(() => stepIndex.value === wizardSteps.length - 1);

  function resetFromSettings(settings: Settings, projects: ProjectView[]): void {
    Object.assign(draft, emptyDraft());
    draft.steps = [...settings.steps];
    draft.mode = settings.mode;
    draft.concurrency = settings.concurrency;
    draft.projectNames = projects.filter((project) => !project.skip && project.exists).map((project) => project.name);
    inspection.value = null;
    inspectedKey.value = "";
    inspectionError.value = "";
    labelTouched.value = false;
    stepIndex.value = 0;
  }

  function ensureInitialized(root: string, settings: Settings, projects: ProjectView[]): void {
    if (initializedRoot.value === root) return;
    resetFromSettings(settings, projects);
    initializedRoot.value = root;
  }

  function setProjects(names: string[]): void {
    draft.projectNames = [...new Set(names)];
  }

  function toggleProject(name: string): void {
    const index = draft.projectNames.indexOf(name);
    if (index >= 0) draft.projectNames.splice(index, 1);
    else draft.projectNames.push(name);
  }

  async function loadCandidates(root: string): Promise<void> {
    const key = [...draft.projectNames].sort().join("|");
    if (key === inspectedKey.value && inspection.value) return;

    await inspectAction.run(async () => {
      const result = await api.inspectProjects(root, draft.projectNames);
      inspection.value = result;
      draft.choices = mergeChoices(draft.choices, result);
      inspectedKey.value = key;
    });
  }

  function addManualPackage(text: string): string | null {
    const spec = parseSpec(text);
    if (!spec) return "libraries.manualInvalid";

    draft.manualPackages = [...draft.manualPackages.filter((entry) => entry.name !== spec.name), spec];
    return null;
  }

  function removeManualPackage(name: string): void {
    draft.manualPackages = draft.manualPackages.filter((entry) => entry.name !== name);
  }

  function refreshLabel(): void {
    if (labelTouched.value) return;
    draft.label = suggestLabel(packages.value.map((entry) => entry.name));
    if (!draft.saveName) draft.saveName = draft.label;
  }

  function setLabel(label: string): void {
    labelTouched.value = true;
    draft.label = label;
  }

  function prefillFromSavedRun(run: SavedRun): void {
    draft.projectNames = [...run.projects];
    draft.manualPackages = run.packages.map((entry) => ({ ...entry }));
    draft.choices = draft.choices.map((choice) => ({ ...choice, selected: false }));
    draft.steps = [...run.steps];
    draft.mode = run.mode;
    draft.concurrency = run.concurrency;
    draft.extraInstallArgs = run.extraInstallArgs.join(" ");
    draft.versionBump = run.version.bump;
    draft.bumpOnlyIfSameAsMain = run.version.onlyIfSameAsMain;
    draft.label = run.name;
    draft.saveRun = false;
    draft.saveName = run.name;
    labelTouched.value = true;
    stepIndex.value = wizardSteps.length - 1;
  }

  function prefillRetry(snapshot: RunSnapshot): void {
    const failed = snapshot.jobs
      .filter((job) => job.status === "failed" || job.status === "skipped")
      .map((job) => job.name);
    const specs = snapshot.packages.map((spec) => parseSpec(spec)).filter((spec): spec is PackageSpec => spec !== null);

    draft.projectNames = failed;
    draft.manualPackages = specs;
    draft.choices = draft.choices.map((choice) => ({ ...choice, selected: false }));
    draft.steps = [...snapshot.steps];
    draft.mode = snapshot.mode;
    draft.concurrency = snapshot.concurrency;
    draft.label = `${snapshot.label}-retry`;
    draft.saveRun = false;
    labelTouched.value = true;
    stepIndex.value = wizardSteps.length - 1;
  }

  function next(): void {
    if (!canProceed.value || isLastStep.value) return;
    stepIndex.value += 1;
    if (stepIndex.value === wizardSteps.length - 1) refreshLabel();
  }

  function back(): void {
    if (stepIndex.value > 0) stepIndex.value -= 1;
  }

  function goTo(index: number): void {
    if (index < stepIndex.value) stepIndex.value = index;
  }

  return {
    stepIndex,
    draft,
    inspection,
    inspecting,
    inspectionError,
    problems,
    canProceed,
    packages,
    plan,
    isLastStep,
    resetFromSettings,
    ensureInitialized,
    setProjects,
    toggleProject,
    loadCandidates,
    addManualPackage,
    removeManualPackage,
    refreshLabel,
    setLabel,
    prefillFromSavedRun,
    prefillRetry,
    next,
    back,
    goTo,
  };
});
