import { describe, expect, it } from "vitest";

import type { DependencyDrift, DriftCell, LibraryDrift } from "@/api/types";
import { filterDrift, selectRow, toggleCell, versionPicks } from "@/lib/dependency-drift";

function cell(project: string, installed: string | null, flags: Partial<DriftCell> = {}): DriftCell {
  return {
    project,
    section: "dependencies",
    declared: "^1.0.0",
    installed,
    outOfRange: false,
    differs: false,
    behind: false,
    ...flags,
  };
}

function library(name: string, cells: DriftCell[], fields: Partial<LibraryDrift> = {}): LibraryDrift {
  return {
    name,
    localProject: null,
    localVersion: null,
    ambiguousProjects: [],
    newestInstalled: null,
    latest: null,
    latestError: null,
    mismatch: cells.some((entry) => entry.outOfRange || entry.differs),
    cells,
    ...fields,
  };
}

const drift: DependencyDrift = {
  projects: ["Shop-Frontend", "Shop-Admin", "Orders-Api"],
  unreadable: [],
  libraries: [
    library("@demo/core", [
      cell("Shop-Frontend", "2.2.0", { differs: true }),
      cell("Orders-Api", "2.1.0", { differs: true, outOfRange: true }),
    ]),
    library("@demo/ui-kit", [cell("Shop-Frontend", "9.0.0"), cell("Shop-Admin", "9.0.0")]),
  ],
};

describe("drift filter", () => {
  it("narrows libraries and project columns by text, case-insensitively", () => {
    const view = filterDrift(drift, { library: "UI", project: "shop", onlyMismatches: false });

    expect(view.projects).toEqual(["Shop-Frontend", "Shop-Admin"]);
    expect(view.libraries.map((entry) => entry.name)).toEqual(["@demo/ui-kit"]);
  });

  it("keeps only libraries with a mismatch among the visible projects", () => {
    expect(filterDrift(drift, { library: "", project: "", onlyMismatches: true }).libraries.map((l) => l.name)).toEqual(
      ["@demo/core"],
    );
    expect(filterDrift(drift, { library: "", project: "admin", onlyMismatches: true }).libraries).toEqual([]);
  });

  it("hides libraries no visible project declares", () => {
    const view = filterDrift(drift, { library: "", project: "orders", onlyMismatches: false });

    expect(view.projects).toEqual(["Orders-Api"]);
    expect(view.libraries.map((entry) => entry.name)).toEqual(["@demo/core"]);
  });
});

describe("version picks", () => {
  it("offers the newest version in use, the latest and the local one, each version once", () => {
    const core = library("@demo/core", [], { newestInstalled: "2.2.0", latest: "2.4.0", localVersion: "2.4.0" });

    expect(versionPicks(core)).toEqual([
      { kind: "newest", version: "2.2.0" },
      { kind: "latest", version: "2.4.0" },
    ]);
    expect(versionPicks(library("left-pad", []))).toEqual([]);
  });
});

describe("drift selection", () => {
  it("collects cells of one library and starts over when a cell of another library is picked", () => {
    let selection = toggleCell(null, "@demo/core", "Shop-Frontend");
    selection = toggleCell(selection, "@demo/core", "Orders-Api");
    expect(selection).toEqual({ library: "@demo/core", projects: ["Shop-Frontend", "Orders-Api"] });

    expect(toggleCell(selection, "@demo/core", "Shop-Frontend")).toEqual({
      library: "@demo/core",
      projects: ["Orders-Api"],
    });
    expect(toggleCell(selection, "@demo/ui-kit", "Shop-Admin")).toEqual({
      library: "@demo/ui-kit",
      projects: ["Shop-Admin"],
    });
    expect(toggleCell({ library: "@demo/core", projects: ["Orders-Api"] }, "@demo/core", "Orders-Api")).toBeNull();
  });

  it("selects a whole row as the visible projects that declare the library", () => {
    const core = drift.libraries[0];
    if (!core) throw new Error("fixture");

    expect(selectRow(core, ["Shop-Frontend", "Shop-Admin"])).toEqual({
      library: "@demo/core",
      projects: ["Shop-Frontend"],
    });
  });
});
