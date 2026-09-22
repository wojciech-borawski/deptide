import type { DependencyCandidate, ExecutionMode, PackageSpec, RunPlan, StepName, VersionBump } from "@/api/types";
import { allSteps } from "@/api/types";
import { applySuffix, isValidSuffix, stripPrerelease } from "./versions";

export type VersionMode = "local" | "branch" | "suffix" | "keep" | "manual";

export interface PackageChoice {
  name: string;
  selected: boolean;
  mode: VersionMode;
  localVersion: string | null;
  localBranch: string | null;
  branchSuffix: string | null;
  currentRanges: string[];
  usedBy: string[];
  saveDev: boolean;
  keepRange: string;
  suffix: string;
  manualVersion: string;
}

export interface Problem {
  key: string;
  name?: string;
}

export interface WizardDraft {
  projectNames: string[];
  choices: PackageChoice[];
  manualPackages: PackageSpec[];
  steps: StepName[];
  mode: ExecutionMode;
  concurrency: number;
  dryRun: boolean;
  extraInstallArgs: string;
  versionBump: VersionBump;
  bumpOnlyIfSameAsMain: boolean;
  label: string;
  saveRun: boolean;
  saveName: string;
}

export function createChoice(candidate: DependencyCandidate): PackageChoice {
  const firstRange = candidate.currentRanges[0] ?? "";

  return {
    name: candidate.name,
    selected: false,
    mode:
      candidate.branchSuffix && candidate.localVersion
        ? "branch"
        : candidate.localVersion
          ? "local"
          : firstRange
            ? "keep"
            : "manual",
    localVersion: candidate.localVersion,
    localBranch: candidate.localBranch,
    branchSuffix: candidate.branchSuffix,
    currentRanges: candidate.currentRanges,
    usedBy: candidate.usedBy,
    saveDev: candidate.isDevDependency,
    keepRange: firstRange,
    suffix: "",
    manualVersion: firstRange || candidate.localVersion || "",
  };
}

export function baseVersion(choice: PackageChoice): string {
  const source = choice.localVersion ?? choice.currentRanges[0] ?? "";
  return source ? stripPrerelease(source) : "";
}

export function resolveVersion(choice: PackageChoice): string {
  switch (choice.mode) {
    case "local":
      return choice.localVersion ?? "";
    case "branch":
      return choice.branchSuffix ? applySuffix(baseVersion(choice), choice.branchSuffix) : "";
    case "suffix":
      return applySuffix(baseVersion(choice), choice.suffix);
    case "keep":
      return choice.keepRange;
    case "manual":
      return choice.manualVersion.trim();
  }
}

export function choiceProblem(choice: PackageChoice): string | null {
  if (!choice.selected) return null;

  if (choice.mode === "suffix") {
    if (!baseVersion(choice)) return "wizard.problems.suffixBase";
    if (!isValidSuffix(choice.suffix)) return "wizard.problems.suffixInvalid";
  }

  if (choice.mode === "local" && !choice.localVersion) return "wizard.problems.noLocal";
  if (choice.mode === "branch" && (!choice.branchSuffix || !baseVersion(choice))) return "wizard.problems.noBranch";
  if (!resolveVersion(choice)) return "wizard.problems.versionRequired";

  return null;
}

export function selectedPackages(draft: WizardDraft): PackageSpec[] {
  const byName = new Map<string, PackageSpec>();

  for (const choice of draft.choices) {
    if (!choice.selected) continue;
    byName.set(choice.name, {
      name: choice.name,
      version: resolveVersion(choice),
      saveDev: choice.saveDev,
    });
  }

  for (const manual of draft.manualPackages) {
    byName.set(manual.name, { ...manual });
  }

  return [...byName.values()];
}

export function parseArguments(text: string): string[] {
  return text
    .split(/\s+/)
    .map((part) => part.trim())
    .filter(Boolean);
}

export function orderedSteps(steps: readonly StepName[]): StepName[] {
  return allSteps.filter((step) => steps.includes(step));
}

export function buildRunPlan(draft: WizardDraft): RunPlan {
  const label = draft.label.trim() || "update";
  const saveName = draft.saveName.trim() || label;

  return {
    projectNames: [...draft.projectNames],
    packages: selectedPackages(draft),
    steps: orderedSteps(draft.steps),
    mode: draft.mode,
    concurrency: Math.max(1, Math.floor(draft.concurrency)),
    dryRun: draft.dryRun,
    extraInstallArgs: parseArguments(draft.extraInstallArgs),
    label,
    saveAs: draft.saveRun ? saveName : null,
    version: { bump: draft.versionBump, onlyIfSameAsMain: draft.bumpOnlyIfSameAsMain },
  };
}

export function buildRerunPlan(plan: RunPlan, projectNames: readonly string[], steps: readonly StepName[]): RunPlan {
  const suffix = "-rerun";
  const base = plan.label.endsWith(suffix) ? plan.label.slice(0, -suffix.length) : plan.label;

  return {
    ...plan,
    projectNames: [...projectNames],
    steps: orderedSteps(steps),
    label: `${base}${suffix}`,
    saveAs: null,
  };
}

export function suggestLabel(packageNames: readonly string[]): string {
  const date = new Date().toISOString().slice(0, 10);
  const first = packageNames[0]?.split("/").pop() ?? "update";
  const extra = packageNames.length > 1 ? `-plus-${packageNames.length - 1}` : "";

  return `${date}-${first}${extra}`;
}

export function draftProblems(draft: WizardDraft, stepIndex: number): Problem[] {
  const problems: Problem[] = [];

  if (stepIndex >= 0 && draft.projectNames.length === 0) {
    problems.push({ key: "wizard.problems.selectProject" });
  }

  if (stepIndex >= 1) {
    if (selectedPackages(draft).length === 0) problems.push({ key: "wizard.problems.selectLibrary" });
    for (const choice of draft.choices) {
      const problem = choiceProblem(choice);
      if (problem) problems.push({ key: problem, name: choice.name });
    }
  }

  if (stepIndex >= 2) {
    if (draft.steps.length === 0) problems.push({ key: "wizard.problems.selectStep" });
    if (!Number.isInteger(draft.concurrency) || draft.concurrency < 1) {
      problems.push({ key: "wizard.problems.concurrency" });
    }
  }

  return problems;
}
