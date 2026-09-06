import { describe, expect, it } from "vitest";

import type { DependencyCandidate } from "@/api/types";
import {
  buildRerunPlan,
  buildRunPlan,
  choiceProblem,
  createChoice,
  draftProblems,
  orderedSteps,
  parseArguments,
  resolveVersion,
  selectedPackages,
  type WizardDraft,
} from "@/lib/wizard-plan";

const core: DependencyCandidate = {
  name: "@acme/core",
  localVersion: "3.1.0-ABC-1",
  localBranch: "ABC-123-widgets",
  branchSuffix: "ABC-123",
  currentRanges: ["3.0.0"],
  usedBy: ["web", "api"],
  isDevDependency: false,
};

const vue: DependencyCandidate = {
  name: "vue",
  localVersion: null,
  localBranch: null,
  branchSuffix: null,
  currentRanges: ["3.5.0"],
  usedBy: ["web"],
  isDevDependency: true,
};

function draft(overrides: Partial<WizardDraft> = {}): WizardDraft {
  return {
    projectNames: ["web"],
    choices: [createChoice(core), createChoice(vue)],
    manualPackages: [],
    steps: ["install", "uninstall"],
    mode: "per-step",
    concurrency: 4,
    dryRun: false,
    extraInstallArgs: "  --force   --legacy-peer-deps ",
    label: "  ",
    saveRun: true,
    saveName: "",
    ...overrides,
  };
}

describe("createChoice", () => {
  it("prefers the branch version, then the local version, then the installed range", () => {
    expect(createChoice(core).mode).toBe("branch");
    expect(createChoice({ ...core, branchSuffix: null }).mode).toBe("local");
    expect(createChoice(vue).mode).toBe("keep");
    expect(createChoice(vue).saveDev).toBe(true);
  });
});

describe("resolveVersion", () => {
  it("resolves every mode", () => {
    const choice = createChoice(core);

    expect(resolveVersion({ ...choice, mode: "local" })).toBe("3.1.0-ABC-1");
    expect(resolveVersion({ ...choice, mode: "branch" })).toBe("3.1.0-ABC-123");
    expect(choiceProblem({ ...choice, selected: true, mode: "branch", branchSuffix: null })).toBe(
      "wizard.problems.noBranch",
    );
    expect(resolveVersion({ ...choice, mode: "suffix", suffix: "ABC-2" })).toBe("3.1.0-ABC-2");
    expect(resolveVersion({ ...choice, mode: "keep" })).toBe("3.0.0");
    expect(resolveVersion({ ...choice, mode: "manual", manualVersion: " 4.0.0 " })).toBe("4.0.0");
  });

  it("reports problems only for selected choices", () => {
    const choice = { ...createChoice(core), selected: true, mode: "suffix" as const, suffix: "bad suffix" };

    expect(choiceProblem(choice)).toBe("wizard.problems.suffixInvalid");
    expect(choiceProblem({ ...choice, selected: false })).toBeNull();
    expect(choiceProblem({ ...choice, suffix: "ABC-3" })).toBeNull();
  });
});

describe("buildRunPlan", () => {
  it("orders steps, trims arguments and derives labels", () => {
    const current = draft();
    current.choices[0]!.selected = true;
    current.manualPackages = [{ name: "left-pad", version: "1.3.0", saveDev: false }];

    const plan = buildRunPlan(current);

    expect(plan.steps).toEqual(["uninstall", "install"]);
    expect(plan.extraInstallArgs).toEqual(["--force", "--legacy-peer-deps"]);
    expect(plan.packages).toEqual([
      { name: "@acme/core", version: "3.1.0-ABC-123", saveDev: false },
      { name: "left-pad", version: "1.3.0", saveDev: false },
    ]);
    expect(plan.label).toBe("update");
    expect(plan.saveAs).toBe("update");
    expect(plan.mode).toBe("per-step");
  });

  it("lets a manual package override a candidate with the same name", () => {
    const current = draft();
    current.choices[0]!.selected = true;
    current.manualPackages = [{ name: "@acme/core", version: "9.9.9", saveDev: false }];

    expect(selectedPackages(current)).toEqual([{ name: "@acme/core", version: "9.9.9", saveDev: false }]);
  });

  it("does not save when the switch is off", () => {
    expect(buildRunPlan(draft({ saveRun: false })).saveAs).toBeNull();
  });
});

describe("draftProblems", () => {
  it("validates each wizard step cumulatively", () => {
    expect(draftProblems(draft({ projectNames: [] }), 0)).toEqual([{ key: "wizard.problems.selectProject" }]);
    expect(draftProblems(draft(), 0)).toEqual([]);
    expect(draftProblems(draft(), 1)).toEqual([{ key: "wizard.problems.selectLibrary" }]);

    const ready = draft({ steps: [], concurrency: 0 });
    ready.choices[0]!.selected = true;
    expect(draftProblems(ready, 2)).toEqual([
      { key: "wizard.problems.selectStep" },
      { key: "wizard.problems.concurrency" },
    ]);
  });
});

describe("buildRerunPlan", () => {
  it("keeps packages and options, narrows projects and steps, and never re-saves", () => {
    const current = draft({ saveRun: true, label: "core" });
    current.choices[0]!.selected = true;
    const plan = buildRunPlan(current);

    const rerun = buildRerunPlan(plan, ["api"], ["build", "install"]);
    expect(rerun.projectNames).toEqual(["api"]);
    expect(rerun.steps).toEqual(["install", "build"]);
    expect(rerun.packages).toEqual(plan.packages);
    expect(rerun.extraInstallArgs).toEqual(plan.extraInstallArgs);
    expect(rerun.label).toBe("core-rerun");
    expect(rerun.saveAs).toBeNull();
    expect(buildRerunPlan(rerun, ["api"], ["build"]).label).toBe("core-rerun");
  });
});

describe("helpers", () => {
  it("parses whitespace separated arguments", () => {
    expect(parseArguments(" --a  --b ")).toEqual(["--a", "--b"]);
    expect(parseArguments("")).toEqual([]);
  });

  it("keeps the canonical step order", () => {
    expect(orderedSteps(["build", "uninstall", "audit"])).toEqual(["uninstall", "audit", "build"]);
    expect(orderedSteps(["audit", "force-install"])).toEqual(["force-install", "audit"]);
  });
});
