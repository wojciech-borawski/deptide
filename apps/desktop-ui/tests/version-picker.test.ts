import { afterEach, describe, expect, it } from "vitest";

import type { DependencyCandidate } from "@/api/types";
import VersionPicker from "@/components/wizard/VersionPicker.vue";
import { i18n } from "@/i18n";
import { createChoice, type PackageChoice } from "@/lib/wizard-plan";
import { mount, textOf, type Mounted, type TestNode } from "./helpers/test-renderer";

const core: DependencyCandidate = {
  name: "@acme/core",
  localVersion: "3.2.0",
  localBranch: null,
  branchSuffix: null,
  currentRanges: ["^3.0.0"],
  usedBy: ["web"],
  isDevDependency: false,
  sections: ["dependencies"],
  libraryProject: "Core",
  ambiguousProjects: [],
};

describe("version picker packing", () => {
  let mounted: Mounted | undefined;

  afterEach(() => mounted?.unmount());

  function show(choice: PackageChoice): string {
    mounted = mount(VersionPicker, (app) => app.use(i18n), { choice });
    return textOf(mounted.root);
  }

  it("offers to pack only a library that is a configured project", () => {
    expect(show({ ...createChoice(core), selected: true })).toContain("Pack from local folder");
    mounted?.unmount();

    expect(show({ ...createChoice({ ...core, libraryProject: null }), selected: true })).not.toContain(
      "Pack from local folder",
    );
  });

  it("says why a library two projects provide cannot be packed", () => {
    const ambiguous = { ...core, libraryProject: null, ambiguousProjects: ["Core", "Core-Copy"] };

    const text = show({ ...createChoice(ambiguous), selected: true });

    expect(text).not.toContain("Pack from local folder");
    expect(text).toContain("Cannot pack: more than one configured project has this package name (Core, Core-Copy)");
  });

  it("groups the two pack modes under one radio name", () => {
    mounted = mount(VersionPicker, (app) => app.use(i18n), {
      choice: { ...createChoice(core), selected: true, mode: "pack" },
    });
    const insidePackModes = (node: TestNode): boolean =>
      node.parent !== null && (node.parent.props.class === "pack-modes" || insidePackModes(node.parent));
    const modes = mounted.root.querySelectorAll('[type="radio"]').filter(insidePackModes);

    expect(modes).toHaveLength(2);
    expect(new Set(modes.map((input) => input.props.name)).size).toBe(1);
    expect(modes[0]?.props.name).toEqual(expect.any(String));
  });

  it("warns that saving the tarball changes package.json", () => {
    const packed: PackageChoice = { ...createChoice(core), selected: true, mode: "pack" };

    expect(show(packed)).not.toContain("points package.json at the tarball");
    mounted?.unmount();

    expect(show({ ...packed, packMode: "save" })).toContain("points package.json at the tarball");
  });
});
