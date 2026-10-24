import { describe, expect, it } from "vitest";

import type { DependencyCandidate } from "@/api/types";
import { allSteps } from "@/api/types";
import {
  buildRerunPlan,
  buildRunPlan,
  changesPackages,
  choiceProblem,
  createChoice,
  draftProblems,
  isPeerOnly,
  orderedSteps,
  parseArguments,
  projectBumpFor,
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
  sections: ["dependencies"],
  libraryProject: "Core",
  ambiguousProjects: [],
};

const vue: DependencyCandidate = {
  name: "vue",
  localVersion: null,
  localBranch: null,
  branchSuffix: null,
  currentRanges: ["3.5.0"],
  usedBy: ["web"],
  isDevDependency: true,
  sections: ["dev", "peer"],
  libraryProject: null,
  ambiguousProjects: [],
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
    versionBump: "minor",
    bumpWhen: "not-bumped-on-branch",
    bumpPerProject: false,
    projectBumps: {},
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

  it("keeps the sections the candidate was found in", () => {
    expect(createChoice(core).sections).toEqual(["dependencies"]);
    expect(createChoice(vue).sections).toEqual(["dev", "peer"]);
  });
});

describe("selectedPackages sections", () => {
  const peerOnly: DependencyCandidate = { ...vue, name: "@acme/ui", isDevDependency: false, sections: ["peer"] };
  const peerAndDependency: DependencyCandidate = { ...core, name: "@acme/theme", sections: ["dependencies", "peer"] };

  it("marks only packages found solely in peerDependencies as peer installs", () => {
    const current = draft({
      choices: [createChoice(core), createChoice(vue), createChoice(peerOnly), createChoice(peerAndDependency)],
    });
    for (const choice of current.choices) choice.selected = true;

    const flags = selectedPackages(current).map(({ name, saveDev, savePeer }) => ({ name, saveDev, savePeer }));

    expect(flags).toEqual([
      { name: "@acme/core", saveDev: false, savePeer: false },
      { name: "vue", saveDev: true, savePeer: false },
      { name: "@acme/ui", saveDev: false, savePeer: true },
      { name: "@acme/theme", saveDev: false, savePeer: false },
    ]);
  });
});

describe("packing a library", () => {
  it("packs the library at its local version, without saving unless asked", () => {
    const choice = { ...createChoice(core), selected: true, mode: "pack" as const };

    expect(selectedPackages(draft({ choices: [choice] }))).toEqual([
      { name: "@acme/core", version: "3.1.0-ABC-1", saveDev: false, savePeer: false, pack: "no-save" },
    ]);
    expect(selectedPackages(draft({ choices: [{ ...choice, packMode: "save" }] }))[0]?.pack).toBe("save");
  });

  it("refuses to pack a library that is not a configured project", () => {
    const choice = { ...createChoice(vue), selected: true, mode: "pack" as const, localVersion: "3.4.0" };

    expect(choiceProblem(choice)).toBe("wizard.problems.packNeedsProject");
  });
});

describe("isPeerOnly", () => {
  it("is true only when every section is peerDependencies", () => {
    expect(isPeerOnly(["peer"])).toBe(true);
    expect(isPeerOnly(["dependencies", "peer"])).toBe(false);
    expect(isPeerOnly(["peer", "dev"])).toBe(false);
    expect(isPeerOnly([])).toBe(false);
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
    current.manualPackages = [{ name: "left-pad", version: "1.3.0", saveDev: false, savePeer: false }];

    const plan = buildRunPlan(current);

    expect(plan.steps).toEqual(["uninstall", "install"]);
    expect(plan.extraInstallArgs).toEqual(["--force", "--legacy-peer-deps"]);
    expect(plan.version).toEqual({ bump: "minor", when: "not-bumped-on-branch" });
    expect(plan.packages).toEqual([
      { name: "@acme/core", version: "3.1.0-ABC-123", saveDev: false, savePeer: false },
      { name: "left-pad", version: "1.3.0", saveDev: false, savePeer: false },
    ]);
    expect(plan.label).toBe("update");
    expect(plan.saveAs).toBe("update");
    expect(plan.mode).toBe("per-step");
  });

  it("lets a manual package override a candidate with the same name", () => {
    const current = draft();
    current.choices[0]!.selected = true;
    current.manualPackages = [{ name: "@acme/core", version: "9.9.9", saveDev: false, savePeer: false }];

    expect(selectedPackages(current)).toEqual([
      { name: "@acme/core", version: "9.9.9", saveDev: false, savePeer: false },
    ]);
  });

  it.each(["always", "same-as-main", "not-bumped-on-branch"] as const)(
    "carries the bump condition %s into the plan",
    (bumpWhen) => {
      expect(buildRunPlan(draft({ bumpWhen })).version).toEqual({ bump: "minor", when: bumpWhen });
    },
  );

  it("sends only the per-project bumps that differ, and only when they are switched on", () => {
    const perProject = draft({
      projectNames: ["web", "api", "cli"],
      steps: ["version"],
      bumpPerProject: true,
      projectBumps: { web: "minor", api: "major", gone: "major" },
    });

    expect(buildRunPlan(perProject).projectBumps).toEqual({ api: "major" });
    expect(buildRunPlan({ ...perProject, bumpPerProject: false }).projectBumps).toEqual({});
    expect(buildRunPlan({ ...perProject, steps: ["build"] }).projectBumps).toEqual({});
  });

  it("keeps the bump of projects named like object members", () => {
    const named = draft({
      projectNames: ["__proto__", "constructor", "toString"],
      steps: ["version"],
      bumpPerProject: true,
      projectBumps: JSON.parse('{"__proto__":"major"}'),
    });

    expect(JSON.stringify(buildRunPlan(named).projectBumps)).toBe('{"__proto__":"major"}');
    expect(projectBumpFor(named, "__proto__")).toBe("major");
    expect(projectBumpFor(named, "constructor")).toBe("minor");
    expect(projectBumpFor(named, "toString")).toBe("minor");
  });

  it("does not save when the switch is off", () => {
    expect(buildRunPlan(draft({ saveRun: false })).saveAs).toBeNull();
  });
});

describe("draftProblems", () => {
  it("validates each wizard step cumulatively", () => {
    expect(draftProblems(draft({ projectNames: [] }), 0)).toEqual([{ key: "wizard.problems.selectProject" }]);
    expect(draftProblems(draft(), 0)).toEqual([]);
    expect(draftProblems(draft(), 1)).toEqual([]);

    const ready = draft({ steps: [], concurrency: 0 });
    ready.choices[0]!.selected = true;
    expect(draftProblems(ready, 2)).toEqual([
      { key: "wizard.problems.selectStep" },
      { key: "wizard.problems.concurrency" },
    ]);
  });
});

describe("a run without libraries", () => {
  it("is allowed when it only bumps, audits or builds", () => {
    expect(draftProblems(draft({ steps: ["version", "audit", "build"] }), 2)).toEqual([]);
  });

  it("flags the steps that need a library", () => {
    expect(draftProblems(draft({ steps: ["install", "build"] }), 2)).toEqual([
      { key: "wizard.problems.stepsNeedLibrary" },
    ]);

    const withLibrary = draft({ steps: ["install", "build"] });
    withLibrary.choices[0]!.selected = true;
    expect(draftProblems(withLibrary, 2)).toEqual([]);
  });

  it("knows which steps change packages", () => {
    expect(allSteps.filter(changesPackages)).toEqual(["uninstall", "install", "force-install"]);
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
    expect(rerun.version).toEqual(plan.version);
    expect(rerun.label).toBe("core-rerun");
    expect(rerun.saveAs).toBeNull();
    expect(buildRerunPlan(rerun, ["api"], ["build"]).label).toBe("core-rerun");
  });
});

describe("buildRerunPlan with per-project bumps", () => {
  it("keeps the per-project bump levels", () => {
    const plan = buildRunPlan(
      draft({
        projectNames: ["web", "api"],
        steps: ["version"],
        bumpPerProject: true,
        projectBumps: { api: "major" },
      }),
    );

    expect(buildRerunPlan(plan, ["api"], ["version"]).projectBumps).toEqual({ api: "major" });
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

  it("runs the audit report after audit fix and before the build", () => {
    expect(orderedSteps(["build", "audit-report", "audit", "install"])).toEqual([
      "install",
      "audit",
      "audit-report",
      "build",
    ]);
  });

  it("lets a run without a library produce only an audit report", () => {
    expect(changesPackages("audit-report")).toBe(false);
    expect(draftProblems(draft({ steps: ["audit-report"] }), 2)).toEqual([]);
  });
});
