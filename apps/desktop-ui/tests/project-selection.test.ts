import { describe, expect, it } from "vitest";
import { ref } from "vue";

import type { ProjectView } from "@/api/types";
import { useProjectFilter } from "@/composables/useProjectFilter";
import { useProjectSelection } from "@/composables/useProjectSelection";

function project(name: string, overrides: Partial<ProjectView> = {}): ProjectView {
  return {
    name,
    path: `../repos/${name}`,
    absolutePath: `C:\\repos\\${name}`,
    exists: true,
    kind: "application",
    skip: false,
    packages: null,
    installArgs: null,
    duplicateOf: [],
    ...overrides,
  };
}

const projects = ref<ProjectView[]>([
  project("apps-web"),
  project("apps-api"),
  project("libs-core", { kind: "library" }),
  project("legacy", { exists: false }),
]);

describe("useProjectFilter", () => {
  it("filters by text on name or path and by kind", () => {
    const filter = useProjectFilter(projects);
    expect(filter.visible.value).toHaveLength(4);

    filter.text.value = "apps";
    expect(filter.visible.value.map((entry) => entry.name)).toEqual(["apps-web", "apps-api"]);

    filter.text.value = "";
    filter.kind.value = "library";
    expect(filter.visible.value.map((entry) => entry.name)).toEqual(["libs-core"]);
  });
});

describe("useProjectSelection", () => {
  function selectionWith(initial: string[]) {
    const names = ref<string[]>([...initial]);
    const filter = useProjectFilter(projects);
    const selection = useProjectSelection(
      { names: () => names.value, replace: (next) => (names.value = next) },
      filter.visible,
    );
    return { selection, filter, names: () => names.value };
  }

  it("toggles single names and reports what is selected", () => {
    const { selection, names } = selectionWith(["apps-web"]);

    expect(selection.isSelected("apps-web")).toBe(true);
    selection.toggle("apps-web");
    selection.toggle("libs-core");
    expect(names()).toEqual(["libs-core"]);
  });

  it("selects and deselects only the visible projects that exist", () => {
    const { selection, filter, names } = selectionWith([]);

    expect(selection.selectable.value.map((entry) => entry.name)).not.toContain("legacy");
    expect(selection.allVisibleSelected.value).toBe(false);

    selection.toggleAllVisible();
    expect(names()).toEqual(["apps-web", "apps-api", "libs-core"]);
    expect(selection.allVisibleSelected.value).toBe(true);

    filter.text.value = "apps";
    selection.toggleAllVisible();
    expect(names()).toEqual(["libs-core"]);
  });
});
