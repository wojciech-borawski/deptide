import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

vi.mock("@/api/commands", () => ({
  inspectProjects: vi.fn(),
  suggestRunLabel: vi.fn(),
  reportError: () => undefined,
}));

import { useWizardStore } from "@/stores/wizard";

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
  });
});
