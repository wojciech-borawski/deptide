import { describe, expect, it } from "vitest";

import { commonPrefixLength, groupByFolder, splitPath } from "@/lib/project-paths";

describe("splitPath", () => {
  it("keeps the drive and drops empty segments on Windows paths", () => {
    expect(splitPath("C:\\example\\repos\\api")).toEqual({
      separator: "\\",
      segments: ["C:", "example", "repos", "api"],
    });
  });

  it("keeps the leading empty segment of an absolute unix path", () => {
    expect(splitPath("/home/dev/api").segments).toEqual(["", "home", "dev", "api"]);
  });

  it("handles relative paths", () => {
    expect(splitPath("../repos/apps/api").segments).toEqual(["..", "repos", "apps", "api"]);
  });
});

describe("commonPrefixLength", () => {
  it("never swallows the last segment of a path", () => {
    expect(
      commonPrefixLength([
        ["C:", "repos", "api"],
        ["C:", "repos", "api", "sub"],
      ]),
    ).toBe(2);
    expect(commonPrefixLength([["C:", "repos", "api"]])).toBe(2);
  });

  it("stops at the first difference", () => {
    expect(
      commonPrefixLength([
        ["C:", "repos", "apps", "api"],
        ["C:", "repos", "libs", "core"],
      ]),
    ).toBe(2);
  });

  it("is zero without paths", () => {
    expect(commonPrefixLength([])).toBe(0);
  });
});

describe("groupByFolder", () => {
  const projects = [
    { name: "apps-api", path: "C:\\repos\\apps\\api" },
    { name: "apps-web", path: "C:\\repos\\apps\\web" },
    { name: "libs-core", path: "C:\\repos\\libs\\core" },
    { name: "tool", path: "C:\\repos\\tool" },
  ];

  it("collapses the shared prefix and groups by the remaining folder", () => {
    const grouped = groupByFolder(projects, (project) => project.path);

    expect(grouped.prefix).toEqual(["C:", "repos"]);
    expect(grouped.separator).toBe("\\");
    expect(grouped.groups.map((group) => group.folder)).toEqual(["apps", "libs", null]);
    expect(grouped.groups[0]?.items.map((entry) => entry.item.name)).toEqual(["apps-api", "apps-web"]);
    expect(grouped.groups[0]?.items[0]?.rest).toEqual(["apps", "api"]);
    expect(grouped.groups[2]?.items[0]?.rest).toEqual(["tool"]);
  });

  it("keeps nested folders together", () => {
    const grouped = groupByFolder([{ path: "/r/apps/v2/api" }, { path: "/r/apps/v2/web" }], (project) => project.path);

    expect(grouped.prefix).toEqual(["", "r", "apps", "v2"]);
    expect(grouped.groups).toHaveLength(1);
    expect(grouped.groups[0]?.folder).toBeNull();
  });

  it("returns nothing for an empty list", () => {
    expect(groupByFolder([], () => "")).toEqual({ separator: "\\", prefix: [], groups: [] });
  });
});
