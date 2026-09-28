import { describe, expect, it } from "vitest";

import {
  buildTree,
  carrySelection,
  checkState,
  comparePaths,
  defaultExpanded,
  fileListKey,
  flattenTree,
  groupByChange,
  nextTabIndex,
  selectedRemovals,
  selectionFor,
  splitByChanges,
  stepThrough,
  toSelection,
  visibleFiles,
  type TreeFolder,
  type TreeNode,
} from "@/lib/receive-view";
import { file, makeProject } from "./helpers/receive-fixtures";

const files = [
  file("README.md", "identical"),
  file("package.json", "replaced"),
  file("src/App.vue", "whitespace"),
  file("src/components/New.vue", "added"),
  file("src/components/Old.vue", "removed"),
  file("src/components/Same.vue", "identical"),
  file("docs/guide.md", "identical"),
];

const project = makeProject("C:\\clip\\web", "web", files);

function folder(nodes: readonly TreeNode[], name: string): TreeFolder {
  const found = nodes.find((node) => node.kind === "folder" && node.name === name);
  if (!found || found.kind !== "folder") throw new Error(`no folder ${name}`);
  return found;
}

function names(nodes: readonly TreeNode[]): string[] {
  return nodes.map((node) => node.name);
}

describe("splitByChanges", () => {
  it("gives a tab to every project with a non-identical file and lists the rest as unchanged", () => {
    const whitespaceOnly = makeProject("C:\\clip\\a", "a", [file("x.ts", "whitespace"), file("y.ts", "identical")]);
    const removedOnly = makeProject("C:\\clip\\b", "b", [file("x.ts", "removed")]);
    const same = makeProject("C:\\clip\\c", "c", [file("x.ts", "identical")]);
    const empty = makeProject("C:\\clip\\d", "d", []);

    const split = splitByChanges([whitespaceOnly, same, removedOnly, empty]);

    expect(split.changed.map((entry) => entry.target)).toEqual(["a", "b"]);
    expect(split.unchanged.map((entry) => entry.target)).toEqual(["c", "d"]);
  });
});

describe("selectionFor", () => {
  it("ticks added, replaced and removed files by default", () => {
    expect(selectionFor(files, "changes")).toEqual([
      "package.json",
      "src/components/New.vue",
      "src/components/Old.vue",
    ]);
  });

  it("ticks every file it is given for all and nothing for none", () => {
    expect(selectionFor(files, "all")).toEqual([
      "README.md",
      "package.json",
      "src/App.vue",
      "src/components/New.vue",
      "src/components/Old.vue",
      "src/components/Same.vue",
      "docs/guide.md",
    ]);
    expect(selectionFor(files, "none")).toEqual([]);
  });

  it("ticks only the visible files for all when identical files are hidden", () => {
    expect(selectionFor(visibleFiles(files, true), "all")).toEqual([
      "package.json",
      "src/App.vue",
      "src/components/New.vue",
      "src/components/Old.vue",
    ]);
  });
});

describe("carrySelection", () => {
  const before = makeProject("C:\\clip\\web", "web", [
    file("kept.ts", "added"),
    file("unticked.ts", "added"),
    file("spaces.ts", "whitespace"),
    file("changed.ts", "whitespace"),
    file("dropped.ts", "replaced"),
  ]);
  const after = makeProject("C:\\clip\\web", "web", [
    file("kept.ts", "added"),
    file("unticked.ts", "added"),
    file("spaces.ts", "whitespace"),
    file("changed.ts", "replaced"),
    file("fresh.ts", "removed"),
  ]);

  it("keeps the ticks of files with the same status and gives the rest their default", () => {
    const ticked = new Set(["kept.ts", "spaces.ts", "dropped.ts"]);

    expect(carrySelection(before, after, ticked)).toEqual(["kept.ts", "spaces.ts", "changed.ts", "fresh.ts"]);
  });

  it("falls back to the default selection for a project that was not in the plan", () => {
    expect(carrySelection(undefined, after, new Set(["spaces.ts"]))).toEqual([
      "kept.ts",
      "unticked.ts",
      "changed.ts",
      "fresh.ts",
    ]);
  });
});

describe("nextTabIndex", () => {
  it("moves right and left, wrapping at both ends", () => {
    expect(nextTabIndex(0, "ArrowRight", 3)).toBe(1);
    expect(nextTabIndex(2, "ArrowRight", 3)).toBe(0);
    expect(nextTabIndex(1, "ArrowLeft", 3)).toBe(0);
    expect(nextTabIndex(0, "ArrowLeft", 3)).toBe(2);
  });

  it("ignores other keys and an empty tab list", () => {
    expect(nextTabIndex(1, "ArrowDown", 3)).toBeNull();
    expect(nextTabIndex(0, "Enter", 3)).toBeNull();
    expect(nextTabIndex(0, "ArrowRight", 0)).toBeNull();
  });
});

describe("visibleFiles", () => {
  it("hides identical files when only affected is on and keeps whitespace files", () => {
    expect(visibleFiles(files, true).map((entry) => entry.relative)).toEqual([
      "package.json",
      "src/App.vue",
      "src/components/New.vue",
      "src/components/Old.vue",
    ]);
    expect(visibleFiles(files, false)).toEqual(files);
  });
});

describe("comparePaths", () => {
  it("compares folder by folder, so a folder sorts before a sibling that extends its name", () => {
    const sorted = ["a-c.ts", "a/b.ts", "B.ts", "a.ts"].sort(comparePaths);
    expect(sorted).toEqual(["B.ts", "a/b.ts", "a-c.ts", "a.ts"]);
  });
});

describe("groupByChange", () => {
  it("groups replaced, whitespace, added, removed, identical, in path order inside a group", () => {
    const groups = groupByChange([
      file("z.ts", "added"),
      file("m.ts", "identical"),
      file("b.ts", "removed"),
      file("a.ts", "added"),
      file("w.ts", "whitespace"),
      file("r.ts", "replaced"),
    ]);

    expect(groups.map((group) => [group.status, group.files.map((entry) => entry.relative)])).toEqual([
      ["replaced", ["r.ts"]],
      ["whitespace", ["w.ts"]],
      ["added", ["a.ts", "z.ts"]],
      ["removed", ["b.ts"]],
      ["identical", ["m.ts"]],
    ]);
  });

  it("leaves out empty groups", () => {
    expect(groupByChange([file("a.ts", "added")]).map((group) => group.status)).toEqual(["added"]);
  });
});

describe("buildTree", () => {
  const tree = buildTree(files);

  it("nests files under their folders, folders first, then files, each by name", () => {
    expect(names(tree)).toEqual(["docs", "src", "README.md", "package.json"]);
    const src = folder(tree, "src");
    expect(src.path).toBe("src");
    expect(names(src.children)).toEqual(["components", "App.vue"]);
    const components = folder(src.children, "components");
    expect(components.path).toBe("src/components");
    expect(names(components.children)).toEqual(["New.vue", "Old.vue", "Same.vue"]);
    expect(components.children[0]).toMatchObject({ kind: "file", path: "src/components/New.vue" });
  });

  it("aggregates status counts and file paths over the whole subtree", () => {
    const src = folder(tree, "src");
    expect(src.counts).toEqual({ added: 1, replaced: 0, whitespace: 1, identical: 1, removed: 1 });
    expect(src.files).toEqual([
      "src/App.vue",
      "src/components/New.vue",
      "src/components/Old.vue",
      "src/components/Same.vue",
    ]);
    expect(folder(tree, "docs").counts).toEqual({ added: 0, replaced: 0, whitespace: 0, identical: 1, removed: 0 });
  });

  it("drops folders with nothing visible once identical files are filtered out", () => {
    const filtered = buildTree(visibleFiles(files, true));
    expect(names(filtered)).toEqual(["src", "package.json"]);
    expect(folder(filtered, "src").files).not.toContain("src/components/Same.vue");
  });
});

describe("checkState", () => {
  const paths = ["a.ts", "b.ts"];

  it("is checked, unchecked or indeterminate depending on how many paths are selected", () => {
    expect(checkState(paths, new Set(["a.ts", "b.ts", "other.ts"]))).toBe("checked");
    expect(checkState(paths, new Set(["other.ts"]))).toBe("unchecked");
    expect(checkState(paths, new Set(["b.ts"]))).toBe("indeterminate");
  });

  it("treats a folder with no files as unchecked", () => {
    expect(checkState([], new Set(["a.ts"]))).toBe("unchecked");
  });
});

describe("defaultExpanded", () => {
  it("expands every folder that holds a non-identical file somewhere below it", () => {
    const expanded = defaultExpanded([
      ...files,
      file("lib/deep/inner/x.ts", "added"),
      file("lib/flat.ts", "identical"),
    ]);
    expect([...expanded].sort()).toEqual(["lib", "lib/deep", "lib/deep/inner", "src", "src/components"]);
  });
});

describe("flattenTree", () => {
  it("lists rows with their depth and skips the children of collapsed folders", () => {
    const tree = buildTree(files);
    const rows = flattenTree(tree, (path) => path === "src");

    expect(rows.map((row) => [row.node.path, row.depth])).toEqual([
      ["docs", 0],
      ["src", 0],
      ["src/components", 1],
      ["src/App.vue", 1],
      ["README.md", 0],
      ["package.json", 0],
    ]);
  });
});

describe("toSelection", () => {
  it("sends ticked removed files as deletions and the rest as files to copy", () => {
    const selection = toSelection(project, new Set(["package.json", "src/components/Old.vue", "src/App.vue"]));

    expect(selection).toEqual({
      source: "C:\\clip\\web",
      target: "web",
      files: ["package.json", "src/App.vue"],
      delete: ["src/components/Old.vue"],
    });
  });

  it("ignores selected paths the plan does not know", () => {
    expect(toSelection(project, new Set(["ghost.ts"]))).toMatchObject({ files: [], delete: [] });
  });
});

describe("selectedRemovals", () => {
  it("collects ticked removed files per project and leaves out projects without any", () => {
    const other = makeProject("C:\\clip\\api", "api", [file("gone.ts", "removed"), file("kept.ts", "removed")]);
    const quiet = makeProject("C:\\clip\\cli", "cli", [file("x.ts", "removed")]);
    const selected: Record<string, Set<string>> = {
      [project.source]: new Set(["src/components/Old.vue", "package.json"]),
      [other.source]: new Set(["gone.ts"]),
      [quiet.source]: new Set(),
    };

    const groups = selectedRemovals([project, other, quiet], (source) => selected[source] ?? new Set());

    expect(groups).toEqual([
      { source: project.source, target: "web", files: ["src/components/Old.vue"] },
      { source: other.source, target: "api", files: ["gone.ts"] },
    ]);
  });
});

describe("stepThrough", () => {
  const order = ["a.ts", "b.ts", "c.ts"];

  it("moves to the next or previous path and stops at the ends", () => {
    expect(stepThrough(order, "a.ts", 1)).toBe("b.ts");
    expect(stepThrough(order, "b.ts", -1)).toBe("a.ts");
    expect(stepThrough(order, "c.ts", 1)).toBe("c.ts");
    expect(stepThrough(order, "a.ts", -1)).toBe("a.ts");
  });

  it("starts at the first or last path when nothing visible is previewed", () => {
    expect(stepThrough(order, null, 1)).toBe("a.ts");
    expect(stepThrough(order, "hidden.ts", -1)).toBe("c.ts");
    expect(stepThrough([], null, 1)).toBeNull();
  });
});

describe("fileListKey", () => {
  const order = ["a.ts", "b.ts", "c.ts"];

  it("moves the cursor with the arrow keys and takes an open preview along", () => {
    expect(fileListKey("ArrowDown", order, { cursor: "a.ts", preview: "a.ts" })).toEqual({
      cursor: "b.ts",
      preview: "b.ts",
    });
    expect(fileListKey("ArrowUp", order, { cursor: "b.ts", preview: "b.ts" })).toEqual({
      cursor: "a.ts",
      preview: "a.ts",
    });
  });

  it("moves only the cursor while the preview is closed", () => {
    expect(fileListKey("ArrowDown", order, { cursor: null, preview: null })).toEqual({ cursor: "a.ts", preview: null });
    expect(fileListKey("ArrowDown", order, { cursor: "b.ts", preview: null })).toEqual({
      cursor: "c.ts",
      preview: null,
    });
  });

  it("starts from the first or last row when the cursor is not visible", () => {
    expect(fileListKey("ArrowUp", order, { cursor: "hidden.ts", preview: null })).toEqual({
      cursor: "c.ts",
      preview: null,
    });
  });

  it("opens the preview for the cursor row with Enter and closes it with Escape", () => {
    expect(fileListKey("Enter", order, { cursor: "b.ts", preview: null })).toEqual({ cursor: "b.ts", preview: "b.ts" });
    expect(fileListKey("Enter", order, { cursor: "b.ts", preview: "a.ts" })).toEqual({
      cursor: "b.ts",
      preview: "b.ts",
    });
    expect(fileListKey("Escape", order, { cursor: "b.ts", preview: "b.ts" })).toEqual({
      cursor: "b.ts",
      preview: null,
    });
  });

  it("leaves keys it has nothing to do for to the browser", () => {
    expect(fileListKey("Enter", order, { cursor: null, preview: null })).toBeNull();
    expect(fileListKey("Enter", order, { cursor: "hidden.ts", preview: null })).toBeNull();
    expect(fileListKey("Escape", order, { cursor: "a.ts", preview: null })).toBeNull();
    expect(fileListKey("ArrowDown", [], { cursor: null, preview: null })).toBeNull();
    expect(fileListKey("Tab", order, { cursor: "a.ts", preview: "a.ts" })).toBeNull();
  });

  it("does nothing with Left and Right in the flat list", () => {
    expect(fileListKey("ArrowLeft", ["src/a.ts"], { cursor: "src/a.ts", preview: null })).toBeNull();
    expect(fileListKey("ArrowRight", ["src/a.ts"], { cursor: "src/a.ts", preview: null })).toBeNull();
  });
});

describe("fileListKey in the tree", () => {
  const order = ["src", "src/a.ts", "src/lib", "src/lib/b.ts", "z.ts"];
  const open = new Map([
    ["src", true],
    ["src/lib", true],
  ]);

  it("stops on folders with the up and down keys and keeps the preview on its file there", () => {
    expect(fileListKey("ArrowDown", order, { cursor: "src/a.ts", preview: "src/a.ts" }, open)).toEqual({
      cursor: "src/lib",
      preview: "src/a.ts",
    });
    expect(fileListKey("ArrowDown", order, { cursor: "src/lib", preview: "src/a.ts" }, open)).toEqual({
      cursor: "src/lib/b.ts",
      preview: "src/lib/b.ts",
    });
    expect(fileListKey("ArrowUp", order, { cursor: "src/a.ts", preview: null }, open)).toEqual({
      cursor: "src",
      preview: null,
    });
  });

  it("expands a collapsed folder with Right, then moves into it", () => {
    const closed = new Map([["src", false]]);

    expect(fileListKey("ArrowRight", ["src", "z.ts"], { cursor: "src", preview: "z.ts" }, closed)).toEqual({
      cursor: "src",
      preview: "z.ts",
      toggle: "src",
    });
    expect(fileListKey("ArrowRight", order, { cursor: "src", preview: "z.ts" }, open)).toEqual({
      cursor: "src/a.ts",
      preview: "src/a.ts",
    });
  });

  it("collapses an expanded folder with Left, and goes up to the parent folder from a file or a collapsed folder", () => {
    expect(fileListKey("ArrowLeft", order, { cursor: "src/lib", preview: null }, open)).toEqual({
      cursor: "src/lib",
      preview: null,
      toggle: "src/lib",
    });
    expect(fileListKey("ArrowLeft", order, { cursor: "src/lib/b.ts", preview: "src/lib/b.ts" }, open)).toEqual({
      cursor: "src/lib",
      preview: "src/lib/b.ts",
    });
    const lib = new Map([
      ["src", true],
      ["src/lib", false],
    ]);
    expect(fileListKey("ArrowLeft", ["src", "src/lib"], { cursor: "src/lib", preview: null }, lib)).toEqual({
      cursor: "src",
      preview: null,
    });
  });

  it("toggles a folder with Enter without touching the preview", () => {
    expect(fileListKey("Enter", order, { cursor: "src/lib", preview: "z.ts" }, open)).toEqual({
      cursor: "src/lib",
      preview: "z.ts",
      toggle: "src/lib",
    });
  });

  it("leaves Left on a top-level row and Right on a file to the browser", () => {
    expect(fileListKey("ArrowLeft", order, { cursor: "z.ts", preview: null }, open)).toBeNull();
    expect(fileListKey("ArrowLeft", order, { cursor: "src", preview: null }, new Map([["src", false]]))).toBeNull();
    expect(fileListKey("ArrowRight", order, { cursor: "src/a.ts", preview: null }, open)).toBeNull();
  });
});
