import { describe, expect, it } from "vitest";

import type { ProjectGroup } from "@/api/types";
import { addGroupToSelection, groupSelection, renameGroup, withGroup, withoutGroup } from "@/lib/project-groups";
import { makeProjectView } from "./helpers/project-fixtures";

const project = makeProjectView;

const available = [project("web"), project("admin"), project("api"), project("legacy", { exists: false })];

function group(name: string, projects: string[]): ProjectGroup {
  return { name, projects };
}

describe("project groups", () => {
  it("selecting a group picks exactly its members this picker can select", () => {
    const frontend = group("frontend", ["admin", "web", "legacy", "removed-app"]);

    expect(groupSelection(frontend, available)).toEqual(["admin", "web"]);
  });

  it("adding a group keeps the current selection and appends the members not in it yet", () => {
    const frontend = group("frontend", ["admin", "web", "legacy"]);

    expect(addGroupToSelection(frontend, available, ["api", "web"])).toEqual(["api", "web", "admin"]);
  });
});

describe("editing groups", () => {
  const frontend = group("frontend", ["web", "admin"]);
  const services = group("services", ["api"]);

  it("saving under a new name appends the group and under an existing name replaces it in place", () => {
    expect(withGroup([frontend], " backend ", ["api"])).toEqual([frontend, group("backend", ["api"])]);
    expect(withGroup([frontend, services], "frontend", ["web"])).toEqual([group("frontend", ["web"]), services]);
  });
});

describe("renaming and deleting groups", () => {
  const frontend = group("frontend", ["web", "admin"]);
  const services = group("services", ["api"]);

  it("renaming keeps the members and the position, deleting removes only the named group", () => {
    expect(renameGroup([frontend, services], "frontend", " web apps ")).toEqual([
      group("web apps", ["web", "admin"]),
      services,
    ]);
    expect(withoutGroup([frontend, services], "frontend")).toEqual([services]);
  });
});
