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
