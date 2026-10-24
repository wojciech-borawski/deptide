import { computed, reactive, ref } from "vue";
import { defineStore } from "pinia";

import * as api from "@/api/commands";
import { describeError } from "@/composables/useAsyncAction";
import type {
  PackageSpec,
  ProjectInspection,
  ProjectView,
  RunPlan,
  RunSnapshot,
  SavedRun,
  Settings,
} from "@/api/types";
import {
  buildRunPlan,
  changesPackages,
  createChoice,
  draftProblems,
  selectedPackages,
  suggestLabel,
  type PackageChoice,
  type WizardDraft,
} from "@/lib/wizard-plan";
import { parseSpec } from "@/lib/versions";

export interface LibraryPrefill {
  projectNames: string[];
  library: string;
  version: string | null;
}

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
    bumpWhen: "always",
    bumpPerProject: false,
    projectBumps: {},
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
      packMode: known.packMode,
      keepRange: known.keepRange || fresh.keepRange,
    };
  });
}

export const useWizardStore = defineStore("wizard", () => {
  const stepIndex = ref(0);
  const draft = reactive<WizardDraft>(emptyDraft());
  const inspection = ref<ProjectInspection | null>(null);
  const inspecting = ref(false);
  const inspectionError = ref("");
  let inspectRequest = 0;
  const inspectedKey = ref("");
  const labelTouched = ref(false);
  const initializedRoot = ref("");
  const pendingLibrary = ref<LibraryPrefill | null>(null);

  const problems = computed(() => draftProblems(draft, stepIndex.value));
  const canProceed = computed(() => problems.value.length === 0);
  const packages = computed(() => selectedPackages(draft));
  const plan = computed(() => buildRunPlan(draft));
  const isLastStep = computed(() => stepIndex.value === wizardSteps.length - 1);

  function replaceDraft(): void {
    inspectRequest += 1;
    inspecting.value = false;
    inspectionError.value = "";
    pendingLibrary.value = null;
  }

  function resetFromSettings(settings: Settings, projects: ProjectView[]): void {
    replaceDraft();
    Object.assign(draft, emptyDraft());
    draft.steps = [...settings.steps];
    draft.mode = settings.mode;
    draft.concurrency = settings.concurrency;
    draft.projectNames = projects.filter((project) => !project.skip && project.exists).map((project) => project.name);
    inspection.value = null;
    inspectedKey.value = "";
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

    const request = ++inspectRequest;
    inspecting.value = true;
    inspectionError.value = "";
    try {
      const result = await api.inspectProjects(root, draft.projectNames);
      if (request !== inspectRequest) return;
      inspection.value = result;
      draft.choices = mergeChoices(draft.choices, result);
      inspectedKey.value = key;
      applyPendingLibrary();
    } catch (cause) {
      if (request === inspectRequest) inspectionError.value = describeError(cause);
    } finally {
      if (request === inspectRequest) inspecting.value = false;
    }
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
    replaceDraft();
    draft.projectNames = [...run.projects];
    draft.manualPackages = run.packages.map((entry) => ({ ...entry }));
    draft.choices = draft.choices.map((choice) => ({ ...choice, selected: false }));
    draft.steps = [...run.steps];
    draft.mode = run.mode;
    draft.concurrency = run.concurrency;
    draft.extraInstallArgs = run.extraInstallArgs.join(" ");
    draft.versionBump = run.version.bump;
    draft.bumpWhen = run.version.when;
    draft.projectBumps = { ...run.projectBumps };
    draft.bumpPerProject = Object.keys(draft.projectBumps).length > 0;
    draft.label = run.name;
    draft.saveRun = false;
    draft.saveName = run.name;
    labelTouched.value = true;
    stepIndex.value = wizardSteps.length - 1;
  }

  function prefillRetry(snapshot: RunSnapshot, plan: RunPlan | null): void {
    const failed = snapshot.jobs
      .filter((job) => job.status === "failed" || job.status === "skipped")
      .map((job) => job.name);
    const specs = snapshot.packages.map((spec) => parseSpec(spec)).filter((spec): spec is PackageSpec => spec !== null);

    replaceDraft();
    draft.projectNames = failed;
    draft.manualPackages = plan ? plan.packages.map((entry) => ({ ...entry })) : specs;
    draft.choices = draft.choices.map((choice) => ({ ...choice, selected: false }));
    draft.steps = [...snapshot.steps];
    draft.mode = snapshot.mode;
    draft.concurrency = snapshot.concurrency;
    if (plan) {
      draft.versionBump = plan.version.bump;
      draft.bumpWhen = plan.version.when;
      draft.projectBumps = { ...plan.projectBumps };
      draft.bumpPerProject = Object.keys(draft.projectBumps).length > 0;
    }
    draft.label = `${snapshot.label}-retry`;
    draft.saveRun = false;
    labelTouched.value = true;
    stepIndex.value = wizardSteps.length - 1;
  }

  function prefillLibrary(request: LibraryPrefill, settings: Settings): void {
    resetFromSettings(settings, []);
    draft.projectNames = [...new Set(request.projectNames)];
    pendingLibrary.value = request;
    stepIndex.value = wizardSteps.indexOf("libraries");
  }

  function applyPendingLibrary(): void {
    const request = pendingLibrary.value;
    if (!request) return;
    pendingLibrary.value = null;

    const candidate = inspection.value?.candidates.find((entry) => entry.name === request.library);
    if (!candidate) {
      if (request.version) {
        draft.manualPackages = [{ name: request.library, version: request.version, saveDev: false, savePeer: false }];
      }
      return;
    }
    const choice = { ...createChoice(candidate), selected: true };
    if (request.version) {
      choice.mode = "manual";
      choice.manualVersion = request.version;
    }
    draft.choices = draft.choices.map((entry) => (entry.name === request.library ? choice : entry));
  }

  function dropStepsThatNeedLibraries(): void {
    if (packages.value.length > 0) return;
    const remaining = draft.steps.filter((step) => !changesPackages(step));
    draft.steps = remaining.length > 0 ? remaining : ["version"];
  }

  function next(): void {
    if (!canProceed.value || isLastStep.value) return;
    stepIndex.value += 1;
    if (wizardSteps[stepIndex.value] === "options") dropStepsThatNeedLibraries();
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
    prefillLibrary,
    next,
    back,
    goTo,
  };
});
