import { describe, expect, it } from "vitest";

import type { DependencySection } from "@/api/types";
import { groupChoicesBySection, otherSections, primarySection, type SectionGroup } from "@/lib/dependency-sections";

function item(name: string, sections: DependencySection[]) {
  return { name, sections };
}

function names(groups: SectionGroup<{ name: string }>[]) {
  return groups.map((group) => [group.field, group.items.map((entry) => entry.name)]);
}

describe("groupChoicesBySection", () => {
  it("lists dependencies, then peerDependencies, then devDependencies", () => {
    const groups = groupChoicesBySection([
      item("typescript", ["dev"]),
      item("vue", ["peer"]),
      item("@acme/core", ["dependencies"]),
    ]);

    expect(names(groups)).toEqual([
      ["dependencies", ["@acme/core"]],
      ["peerDependencies", ["vue"]],
      ["devDependencies", ["typescript"]],
    ]);
    expect(groups.map((group) => group.section)).toEqual(["dependencies", "peer", "dev"]);
  });

  it("puts each package once, under its section with the highest precedence", () => {
    const groups = groupChoicesBySection([
      item("tooling", ["dev", "peer"]),
      item("shared", ["dev", "dependencies", "peer"]),
      item("vitest", ["dev"]),
    ]);

    expect(names(groups)).toEqual([
      ["dependencies", ["shared"]],
      ["peerDependencies", ["tooling"]],
      ["devDependencies", ["vitest"]],
    ]);
  });

  it("omits empty groups and keeps the incoming order inside a group", () => {
    const groups = groupChoicesBySection([
      item("zod", ["dev"]),
      item("@acme/ui", ["dev", "peer"]),
      item("alpha", ["dev"]),
      item("@acme/core", ["peer"]),
    ]);

    expect(names(groups)).toEqual([
      ["peerDependencies", ["@acme/ui", "@acme/core"]],
      ["devDependencies", ["zod", "alpha"]],
    ]);
    expect(groupChoicesBySection([])).toEqual([]);
  });
});

describe("section helpers", () => {
  it("name the primary section and the others in precedence order", () => {
    expect(primarySection(["dev", "peer"])).toBe("peer");
    expect(otherSections(["dev", "peer"])).toEqual(["dev"]);
    expect(otherSections(["dev", "dependencies", "peer"])).toEqual(["peer", "dev"]);
    expect(otherSections(["dev"])).toEqual([]);
  });
});
