import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

vi.mock("@/api/commands", () => ({
  inspectProjects: vi.fn(),
  suggestRunLabel: vi.fn(),
  reportError: () => undefined,
}));

import * as api from "@/api/commands";
import type { DependencyCandidate, ProjectInspection, RunPlan, RunSnapshot, SavedRun, Settings } from "@/api/types";
import { useWizardStore } from "@/stores/wizard";

const settings: Settings = {
  projectsRoot: "C:\\ws",
  scanDepth: 3,
  concurrency: 4,
  steps: ["install", "build"],
  mode: "per-project",
  extraIgnoredDirectories: [],
  branchSuffixPattern: "",
};

const auditRun: SavedRun = {
  name: "saved-audit",
  savedAt: "2026-01-01T00:00:00Z",
  projects: ["api"],
  packages: [{ name: "saved-lib", version: "1.0.0", saveDev: false, savePeer: false }],
  steps: ["audit-report"],
  mode: "per-step",
  concurrency: 1,
  extraInstallArgs: ["--legacy-peer-deps"],
  version: { bump: "minor", when: "same-as-main" },
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => (resolve = done));
  return { promise, resolve };
}

function candidate(
  name: string,
  localVersion: string | null,
  libraryProject: string | null = null,
): DependencyCandidate {
  return {
    name,
    localVersion,
    localBranch: null,
    branchSuffix: null,
    currentRanges: ["^3.0.0"],
    usedBy: ["web", "api"],
    isDevDependency: false,
    sections: ["dependencies"],
    libraryProject,
    ambiguousProjects: [],
  };
}

describe("wizard project selection", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it("replaces and toggles the selected project names", () => {
    const wizard = useWizardStore();

    wizard.setProjects(["web", "api"]);
    expect(wizard.draft.projectNames).toEqual(["web", "api"]);

    wizard.toggleProject("api");
    wizard.toggleProject("core");
    expect(wizard.draft.projectNames).toEqual(["web", "core"]);
  });

  it("does not keep duplicates when the same name is set twice", () => {
    const wizard = useWizardStore();

    wizard.setProjects(["web", "web"]);
    wizard.toggleProject("web");
    expect(wizard.draft.projectNames).not.toContain("web");
  });
});

describe("wizard prefill", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it("starts with an unconditional bump and takes the condition from a saved run", () => {
    const wizard = useWizardStore();
    expect(wizard.draft.bumpWhen).toBe("always");

    wizard.prefillFromSavedRun({
      name: "saved",
      savedAt: "2026-01-01T00:00:00Z",
      projects: ["web"],
      packages: [{ name: "left-pad", version: "1.3.0", saveDev: false, savePeer: false }],
      steps: ["version"],
      concurrency: 2,
      mode: "per-project",
      extraInstallArgs: [],
      version: { bump: "minor", when: "same-as-main" },
    });

    expect(wizard.draft.versionBump).toBe("minor");
    expect(wizard.draft.bumpWhen).toBe("same-as-main");
    expect(wizard.draft.bumpPerProject).toBe(false);
  });

  it("takes per-project bumps from a saved run and switches them on", () => {
    const wizard = useWizardStore();

    wizard.prefillFromSavedRun({
      name: "saved",
      savedAt: "2026-01-01T00:00:00Z",
      projects: ["web", "api"],
      packages: [],
      steps: ["version"],
      concurrency: 2,
      mode: "per-project",
      extraInstallArgs: [],
      version: { bump: "patch", when: "not-bumped-on-branch" },
      projectBumps: { api: "major" },
    });

    expect(wizard.draft.bumpPerProject).toBe(true);
    expect(wizard.draft.projectBumps).toEqual({ api: "major" });
    expect(wizard.plan.projectBumps).toEqual({ api: "major" });
  });
});

describe("wizard retry", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it("keeps the bump policy of the run it retries", () => {
    const wizard = useWizardStore();
    const snapshot = {
      label: "bump",
      packages: [],
      steps: ["version"],
      mode: "per-project",
      concurrency: 2,
      jobs: [
        { name: "web", status: "failed" },
        { name: "api", status: "skipped" },
      ],
    } as unknown as RunSnapshot;
    const plan: RunPlan = {
      projectNames: ["web", "api"],
      packages: [],
      steps: ["version"],
      mode: "per-project",
      concurrency: 2,
      dryRun: false,
      extraInstallArgs: [],
      label: "bump",
      saveAs: null,
      version: { bump: "minor", when: "not-bumped-on-branch" },
      projectBumps: { api: "major" },
    };

    wizard.prefillRetry(snapshot, plan);

    expect(wizard.plan.version).toEqual({ bump: "minor", when: "not-bumped-on-branch" });
    expect(wizard.plan.projectBumps).toEqual({ api: "major" });
  });

  it("keeps the pack choice of the run it retries", () => {
    const wizard = useWizardStore();
    const snapshot = {
      label: "pack",
      packages: ["@acme/core@3.2.0"],
      steps: ["install"],
      mode: "per-project",
      concurrency: 2,
      jobs: [{ name: "web", status: "failed" }],
    } as unknown as RunSnapshot;
    const packed = { name: "@acme/core", version: "3.2.0", saveDev: false, savePeer: false, pack: "save" as const };
    const plan: RunPlan = {
      projectNames: ["web"],
      packages: [packed],
      steps: ["install"],
      mode: "per-project",
      concurrency: 2,
      dryRun: false,
      extraInstallArgs: [],
      label: "pack",
      saveAs: null,
      version: { bump: "patch", when: "always" },
      projectBumps: {},
    };

    wizard.prefillRetry(snapshot, plan);

    expect(wizard.plan.packages).toEqual([packed]);
  });
});

describe("wizard without libraries", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it("drops the steps that need a library when leaving the libraries step without one", () => {
    const wizard = useWizardStore();
    wizard.setProjects(["web"]);
    wizard.draft.steps = ["uninstall", "install", "version", "audit", "build"];

    wizard.next();
    wizard.next();

    expect(wizard.stepIndex).toBe(2);
    expect(wizard.draft.steps).toEqual(["version", "audit", "build"]);
  });

  it("offers the version step when nothing else is left", () => {
    const wizard = useWizardStore();
    wizard.setProjects(["web"]);
    wizard.draft.steps = ["uninstall", "install"];

    wizard.next();
    wizard.next();

    expect(wizard.draft.steps).toEqual(["version"]);
  });

  it("keeps the install steps when a library is chosen", () => {
    const wizard = useWizardStore();
    wizard.setProjects(["web"]);
    wizard.draft.steps = ["uninstall", "install", "build"];
    wizard.draft.manualPackages = [{ name: "left-pad", version: "1.3.0", saveDev: false, savePeer: false }];

    wizard.next();
    wizard.next();

    expect(wizard.stepIndex).toBe(2);
    expect(wizard.draft.steps).toEqual(["uninstall", "install", "build"]);
  });
});

describe("wizard packed libraries", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.mocked(api.inspectProjects).mockResolvedValue({
      candidates: [candidate("@acme/core", "3.2.0", "Core")],
      unreadable: [],
    });
  });

  it("keeps the pack choice when the candidates are loaded again", async () => {
    const wizard = useWizardStore();
    wizard.setProjects(["web"]);
    await wizard.loadCandidates("C:\\ws");
    const core = wizard.draft.choices[0];
    if (core) Object.assign(core, { selected: true, mode: "pack", packMode: "save" });

    wizard.setProjects(["web", "api"]);
    await wizard.loadCandidates("C:\\ws");

    expect(wizard.packages).toEqual([
      { name: "@acme/core", version: "3.2.0", saveDev: false, savePeer: false, pack: "save" },
    ]);
  });
});

describe("wizard prefill from the dependencies screen", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.mocked(api.inspectProjects).mockClear();
    vi.mocked(api.inspectProjects).mockResolvedValue({
      candidates: [candidate("@acme/core", "3.2.0"), candidate("@acme/ui", null)],
      unreadable: [],
    });
  });

  it("selects the projects and the library with its version and opens the libraries step", async () => {
    const wizard = useWizardStore();
    wizard.setProjects(["web", "api"]);
    await wizard.loadCandidates("C:\\ws");
    const ui = wizard.draft.choices.find((choice) => choice.name === "@acme/ui");
    if (ui) ui.selected = true;
    wizard.draft.manualPackages = [{ name: "left-pad", version: "1.0.0", saveDev: false, savePeer: false }];

    wizard.prefillLibrary({ projectNames: ["web", "api"], library: "@acme/core", version: "3.1.4" }, settings);
    await wizard.loadCandidates("C:\\ws");

    expect(vi.mocked(api.inspectProjects)).toHaveBeenCalledTimes(2);
    expect(wizard.stepIndex).toBe(1);
    expect(wizard.draft.projectNames).toEqual(["web", "api"]);
    expect(wizard.packages).toEqual([{ name: "@acme/core", version: "3.1.4", saveDev: false, savePeer: false }]);
  });

  it("keeps the library's proposed version when no version was picked", async () => {
    const wizard = useWizardStore();

    wizard.prefillLibrary({ projectNames: ["web"], library: "@acme/core", version: null }, settings);
    await wizard.loadCandidates("C:\\ws");

    expect(wizard.packages).toEqual([{ name: "@acme/core", version: "3.2.0", saveDev: false, savePeer: false }]);
  });
});

describe("wizard library handoff", () => {
  const core = candidate("@acme/core", "3.2.0");
  const ui = candidate("@acme/ui", "1.4.0");

  beforeEach(() => {
    setActivePinia(createPinia());
    vi.mocked(api.inspectProjects).mockReset();
    vi.mocked(api.inspectProjects).mockResolvedValue({ candidates: [core, ui], unreadable: [] });
  });

  it("replaces a loaded saved run with a fresh update draft from the settings", async () => {
    const wizard = useWizardStore();
    wizard.resetFromSettings(settings, []);
    wizard.prefillFromSavedRun(auditRun);

    wizard.prefillLibrary({ projectNames: ["web"], library: "@acme/core", version: "3.2.0" }, settings);
    await wizard.loadCandidates("C:\\ws");

    expect(wizard.plan.steps).toEqual(["install", "build"]);
    expect(wizard.plan.mode).toBe("per-project");
    expect(wizard.plan.concurrency).toBe(4);
    expect(wizard.plan.extraInstallArgs).toEqual([]);
    expect(wizard.plan.version).toEqual({ bump: "patch", when: "always" });
    expect(wizard.packages).toEqual([{ name: "@acme/core", version: "3.2.0", saveDev: false, savePeer: false }]);
    expect([wizard.draft.label, wizard.draft.saveRun]).toEqual(["", true]);
  });

  it("proposes the current version again when an earlier handoff picked one by hand", async () => {
    const wizard = useWizardStore();
    wizard.prefillLibrary({ projectNames: ["web"], library: "@acme/core", version: "0.1.0" }, settings);
    await wizard.loadCandidates("C:\\ws");

    wizard.prefillLibrary({ projectNames: ["web"], library: "@acme/core", version: null }, settings);
    await wizard.loadCandidates("C:\\ws");

    expect(wizard.packages).toEqual([{ name: "@acme/core", version: "3.2.0", saveDev: false, savePeer: false }]);
  });

  it("forgets a pending handoff on Start over", async () => {
    const wizard = useWizardStore();
    wizard.prefillLibrary({ projectNames: ["web"], library: "@acme/core", version: "0.1.0" }, settings);

    wizard.resetFromSettings(settings, []);
    wizard.setProjects(["api"]);
    await wizard.loadCandidates("C:\\ws");

    expect(wizard.packages).toEqual([]);
  });

  for (const [label, replace] of [
    ["a saved run", (wizard: ReturnType<typeof useWizardStore>) => wizard.prefillFromSavedRun(auditRun)],
    [
      "a retry",
      (wizard: ReturnType<typeof useWizardStore>) =>
        wizard.prefillRetry(
          {
            label: "retry-source",
            packages: ["retry-lib@2.0.0"],
            steps: ["install"],
            mode: "per-project",
            concurrency: 2,
            jobs: [{ name: "api", status: "failed" }],
          } as RunSnapshot,
          null,
        ),
    ],
    ["Start over", (wizard: ReturnType<typeof useWizardStore>) => wizard.resetFromSettings(settings, [])],
  ] as const) {
    it(`ignores the inspection of a handoff that ${label} replaced while it ran`, async () => {
      const answer = deferred<ProjectInspection>();
      vi.mocked(api.inspectProjects).mockReturnValueOnce(answer.promise);
      const wizard = useWizardStore();
      wizard.prefillLibrary({ projectNames: ["web"], library: "@acme/core", version: "0.1.0" }, settings);
      const pending = wizard.loadCandidates("C:\\ws");

      replace(wizard);
      const before = wizard.packages.map((entry) => entry.name);
      answer.resolve({ candidates: [core, ui], unreadable: [] });
      await pending;

      expect(wizard.inspecting).toBe(false);
      expect(wizard.inspection).toBeNull();
      expect(wizard.packages.map((entry) => entry.name)).toEqual(before);
    });
  }

  it("lets a second handoff win over the first one's late inspection", async () => {
    const first = deferred<ProjectInspection>();
    const second = deferred<ProjectInspection>();
    vi.mocked(api.inspectProjects).mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    const wizard = useWizardStore();
    wizard.prefillLibrary({ projectNames: ["web"], library: "@acme/core", version: "0.1.0" }, settings);
    const firstLoad = wizard.loadCandidates("C:\\ws");
    wizard.prefillLibrary({ projectNames: ["api"], library: "@acme/ui", version: null }, settings);
    const secondLoad = wizard.loadCandidates("C:\\ws");

    second.resolve({ candidates: [ui], unreadable: [] });
    await secondLoad;
    first.resolve({ candidates: [core, ui], unreadable: [] });
    await firstLoad;

    expect(wizard.draft.projectNames).toEqual(["api"]);
    expect(wizard.inspection?.candidates.map((entry) => entry.name)).toEqual(["@acme/ui"]);
    expect(wizard.packages).toEqual([{ name: "@acme/ui", version: "1.4.0", saveDev: false, savePeer: false }]);
  });
});
