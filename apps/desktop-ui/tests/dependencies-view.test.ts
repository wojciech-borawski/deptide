import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { type App } from "vue";
import { createPinia, setActivePinia, type Pinia } from "pinia";

vi.mock("@/api/commands", () => ({
  reportError: () => undefined,
  readDependencyDrift: vi.fn(() => new Promise(() => {})),
  readDependencyGraph: vi.fn(() => new Promise(() => {})),
  findInstalledCopies: vi.fn(() => new Promise(() => {})),
  openDependencyRequest: vi.fn(async () => 1),
  cancelDependencyRequest: vi.fn(),
}));
vi.mock("@/components/layout/PageHeader.vue", () => ({ default: { render: () => null } }));
vi.mock("@/components/dependencies/VersionDriftTab.vue", () => ({ default: { render: () => null } }));
vi.mock("@/components/dependencies/DependencyGraphTab.vue", () => ({ default: { render: () => null } }));
vi.mock("@/components/dependencies/DuplicateCopiesTab.vue", () => ({ default: { render: () => null } }));

import * as api from "@/api/commands";
import { i18n } from "@/i18n";
import { useDependenciesStore } from "@/stores/dependencies";
import { useWorkspaceStore } from "@/stores/workspace";
import DependenciesView from "@/views/DependenciesView.vue";
import { mount, testDocument } from "./helpers/test-renderer";

let pinia: Pinia;

function install(app: App): void {
  app.use(pinia);
  app.use(i18n);
  app.directive("ripple", {});
}

beforeEach(() => {
  pinia = createPinia();
  setActivePinia(pinia);
  vi.clearAllMocks();
  vi.stubGlobal("document", testDocument);
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("dependencies screen", () => {
  it("cancels its running drift request when the screen is left", async () => {
    useWorkspaceStore().snapshot = { root: "D:/work" } as never;
    const mounted = mount(DependenciesView as never, install as never, {});
    await new Promise((resolve) => setTimeout(resolve, 0));
    const [root, checkLatest, requestId] = vi.mocked(api.readDependencyDrift).mock.calls[0] ?? [];

    mounted.unmount();

    expect([root, checkLatest]).toEqual(["D:/work", false]);
    expect(vi.mocked(api.cancelDependencyRequest).mock.calls).toEqual([[requestId]]);
    expect(useDependenciesStore().driftBusy).toBe(false);
  });
});
