import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { type App } from "vue";
import { createPinia, setActivePinia, type Pinia } from "pinia";

vi.mock("vue-router", () => ({ useRouter: () => ({ push: vi.fn() }) }));
vi.mock("@/router", () => ({ routeNames: { wizard: "wizard" } }));

import VersionDriftTab from "@/components/dependencies/VersionDriftTab.vue";
import { i18n } from "@/i18n";
import { useDependenciesStore } from "@/stores/dependencies";
import { mount, testDocument, textOf, type Mounted } from "./helpers/test-renderer";

let pinia: Pinia;
let mounted: Mounted | undefined;

function install(app: App): void {
  app.use(pinia);
  app.use(i18n);
  app.directive("ripple", {});
}

beforeEach(() => {
  pinia = createPinia();
  setActivePinia(pinia);
  vi.stubGlobal("document", testDocument);
});

afterEach(() => {
  mounted?.unmount();
  vi.unstubAllGlobals();
});

describe("version drift tab", () => {
  it("names the projects that make a library's local version ambiguous", () => {
    useDependenciesStore().drift = {
      projects: ["Core", "Core-Copy", "Web"],
      unreadable: [],
      libraries: [
        {
          name: "@acme/core",
          localProject: null,
          localVersion: null,
          ambiguousProjects: ["Core", "Core-Copy"],
          newestInstalled: null,
          latest: null,
          latestError: null,
          mismatch: false,
          cells: [
            {
              project: "Web",
              section: "dependencies",
              declared: "^3.0.0",
              installed: "3.1.0",
              outOfRange: false,
              differs: false,
              behind: false,
            },
          ],
        },
      ],
    };

    mounted = mount(VersionDriftTab as never, install as never, {});

    expect(textOf(mounted.root)).toContain("local version ambiguous: Core, Core-Copy");
  });
});
