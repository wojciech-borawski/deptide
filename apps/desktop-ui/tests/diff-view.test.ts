import { describe, expect, it } from "vitest";

import type { LineChunk } from "@/api/types";
import {
  changeRuns,
  chunksOf,
  chunksOverlapping,
  diffTexts,
  hasChanges,
  lineDiff,
  languageFor,
  splitLines,
  rowCount,
  rowLimit,
  splitRows,
  tooManyRows,
  unifiedRows,
  wholeFile,
  type DiffLine,
  type LineSide,
  type SplitRow,
  type UnifiedRow,
} from "@/lib/diff-view";

function numbered(count: number, prefix = "line"): string[] {
  return Array.from({ length: count }, (_, index) => `${prefix} ${index + 1}`);
}

function text(lines: readonly string[]): string {
  return lines.map((line) => `${line}\n`).join("");
}

function describeUnified(rows: readonly UnifiedRow[]): string[] {
  return rows.map((row) => {
    if (row.kind === "gap") return `gap ${row.id} x${row.count}`;
    const { line } = row;
    const sign = line.kind === "added" ? "+" : line.kind === "removed" ? "-" : " ";
    return `${sign}${line.old?.number ?? "."}:${line.new?.number ?? "."} ${(line.new ?? line.old)?.text ?? ""}`;
  });
}

function describeSplit(rows: readonly SplitRow[]): string[] {
  return rows.map((row) => {
    if (row.kind === "gap") return `gap ${row.id} x${row.count}`;
    const cell = (value: SplitRow & { kind: "pair" }, side: "left" | "right") => {
      const found = value[side];
      if (!found) return "_";
      const sign = found.kind === "added" ? "+" : found.kind === "removed" ? "-" : " ";
      return `${sign}${found.side.number} ${found.side.text}`;
    };
    return `${cell(row, "left")} | ${cell(row, "right")}`;
  });
}

function changed(lines: readonly DiffLine[]): DiffLine[] {
  return lines.filter((line) => line.kind !== "context");
}

describe("splitLines", () => {
  it("keeps each line's ending and knows when the last line has none", () => {
    expect(splitLines("")).toEqual([]);
    expect(splitLines("a\r\nb\nc\rd")).toEqual([
      { text: "a", ending: "crlf" },
      { text: "b", ending: "lf" },
      { text: "c", ending: "cr" },
      { text: "d", ending: "none" },
    ]);
    expect(splitLines("a\n\n")).toEqual([
      { text: "a", ending: "lf" },
      { text: "", ending: "lf" },
    ]);
  });
});

describe("diffTexts with collapsed context", () => {
  const before = numbered(20);
  const after = [...before];
  after[9] = "changed 10";

  it("shows three lines of context around a change and folds the rest into gaps", () => {
    const lines = diffTexts(text(before), text(after));

    expect(describeUnified(unifiedRows(lines, new Set()))).toEqual([
      "gap 0 x6",
      " 7:7 line 7",
      " 8:8 line 8",
      " 9:9 line 9",
      "-10:. line 10",
      "+.:10 changed 10",
      " 11:11 line 11",
      " 12:12 line 12",
      " 13:13 line 13",
      "gap 14 x7",
    ]);
  });

  it("expands one gap at a time, all of its lines at once", () => {
    const lines = diffTexts(text(before), text(after));

    const rows = describeUnified(unifiedRows(lines, new Set([14])));

    expect(rows[0]).toBe("gap 0 x6");
    expect(rows.slice(-7)).toEqual(
      Array.from({ length: 7 }, (_, index) => ` ${14 + index}:${14 + index} line ${14 + index}`),
    );
    expect(rows).toHaveLength(16);
  });

  it("keeps changes whose context overlaps in one block", () => {
    const edited = [...before];
    edited[4] = "changed 5";
    edited[10] = "changed 11";

    const rows = describeUnified(unifiedRows(diffTexts(text(before), text(edited)), new Set()));

    expect(rows[0]).toBe("gap 0 x1");
    expect(rows.filter((row) => row.startsWith("gap"))).toEqual(["gap 0 x1", "gap 16 x6"]);
    expect(rows).toContain(" 8:8 line 8");
  });

  it("shows every line when asked to", () => {
    const rows = unifiedRows(diffTexts(text(before), text(after)), "all");

    expect(rows.every((row) => row.kind === "line")).toBe(true);
    expect(rows).toHaveLength(21);
  });

  it("folds a file without changes into one gap", () => {
    const lines = diffTexts(text(before), text(before));

    expect(hasChanges(lines)).toBe(false);
    expect(describeUnified(unifiedRows(lines, new Set()))).toEqual(["gap 0 x20"]);
  });
});

describe("side-by-side rows", () => {
  it("pairs removed and added lines and pads the shorter side", () => {
    const before = ["a", "old 1", "old 2", "old 3", "b"];
    const after = ["a", "new 1", "b", "c"];

    const rows = describeSplit(splitRows(diffTexts(text(before), text(after)), "all"));

    expect(rows).toEqual([
      " 1 a |  1 a",
      "-2 old 1 | +2 new 1",
      "-3 old 2 | _",
      "-4 old 3 | _",
      " 5 b |  3 b",
      "_ | +4 c",
    ]);
  });

  it("uses the same gaps as the unified view", () => {
    const before = numbered(12);
    const after = [...before, "tail"];
    const lines = diffTexts(text(before), text(after));

    expect(describeSplit(splitRows(lines, new Set()))).toEqual([
      "gap 0 x9",
      " 10 line 10 |  10 line 10",
      " 11 line 11 |  11 line 11",
      " 12 line 12 |  12 line 12",
      "_ | +13 tail",
    ]);
    expect(unifiedRows(lines, new Set())[0]).toEqual({ kind: "gap", id: 0, count: 9 });
  });
});

describe("whitespace", () => {
  const before = "const a = 1;\r\nconst b = 2;\r\n  return a;\r\n";
  const after = "const a = 1;\nconst b = 2;   \n  return a;\n";

  it("shows line ending and trailing space changes when whitespace counts", () => {
    const lines = diffTexts(before, after);

    expect(changed(lines)).toHaveLength(6);
    const first = lines.find((line) => line.kind === "removed");
    expect(first?.old).toMatchObject({ number: 1, text: "const a = 1;", ending: "crlf", endingChanged: true });
    const second = lines.filter((line) => line.kind === "added")[1];
    expect(second?.new).toMatchObject({
      text: "const b = 2;   ",
      endingChanged: true,
      words: [{ start: 12, end: 15 }],
    });
  });

  it("treats lines that differ only in whitespace or line endings as equal when hidden", () => {
    const lines = diffTexts(before, after, { ignoreWhitespace: true });

    expect(hasChanges(lines)).toBe(false);
    expect(lines).toHaveLength(3);
    expect(lines[1]?.old?.text).toBe("const b = 2;");
    expect(lines[1]?.new?.text).toBe("const b = 2;   ");
    expect(lines.every((line) => !line.old?.endingChanged && !line.new?.endingChanged)).toBe(true);
  });

  it("ignores indentation and blank lines that were only added or removed", () => {
    const lines = diffTexts("if (a) {\nb();\n}\n", "if (a) {\n\n    b();\n\n}\n", { ignoreWhitespace: true });

    expect(hasChanges(lines)).toBe(false);
    expect(describeUnified(unifiedRows(lines, "all"))).toEqual([
      " 1:1 if (a) {",
      " .:2 ",
      " 2:3     b();",
      " .:4 ",
      " 3:5 }",
    ]);
  });

  it("still shows real changes next to hidden whitespace", () => {
    const lines = diffTexts("a = 1\r\nb = 2\r\n", "a = 1\nb = 3\n", { ignoreWhitespace: true });

    expect(describeUnified(unifiedRows(lines, "all"))).toEqual([" 1:1 a = 1", "-2:. b = 2", "+.:2 b = 3"]);
    expect(changed(lines).map((line) => (line.old ?? line.new)?.endingChanged)).toEqual([false, false]);
  });

  it("ignores whitespace inside a line when hidden, like git diff -w", () => {
    const lines = diffTexts("a=1\nf(x,y)\n", "a = 1\nf( x, y )\n", { ignoreWhitespace: true });

    expect(hasChanges(lines)).toBe(false);
  });

  it("marks only the non-whitespace words of a changed pair when hidden", () => {
    const lines = diffTexts("a  =  1\r\n", "a = 2\n", { ignoreWhitespace: true });

    const [removed, added] = changed(lines);
    const marked = (side: LineSide | null | undefined) =>
      side?.words.map((range) => side.text.slice(range.start, range.end)) ?? [];
    expect(marked(removed?.old).map((part) => part.trim())).toEqual(["1"]);
    expect(marked(added?.new).map((part) => part.trim())).toEqual(["2"]);
    expect(removed?.old?.endingChanged).toBe(false);
    expect(added?.new?.endingChanged).toBe(false);
  });

  it("puts blank lines that only one side has on their own side in split view", () => {
    const lines = diffTexts("if (a) {\nb();\n}\n", "if (a) {\n\n    b();\n\n}\n", { ignoreWhitespace: true });

    expect(describeSplit(splitRows(lines, "all"))).toEqual([
      " 1 if (a) { |  1 if (a) {",
      "_ |  2 ",
      " 2 b(); |  3     b();",
      "_ |  4 ",
      " 3 } |  5 }",
    ]);
  });
});

describe("large inputs", () => {
  it("gives up on a line diff with more than about 1000 edits and shows one removed and one added block", () => {
    const before = numbered(1200);
    const after = before.map((line, index) => (index % 2 === 0 ? `${line} changed` : line));

    const lines = diffTexts(text(before), text(after));

    const kinds = lines.map((line) => line.kind);
    const firstAdded = kinds.indexOf("added");
    expect(kinds.slice(0, firstAdded).every((kind) => kind === "removed")).toBe(true);
    expect(kinds.slice(firstAdded).every((kind) => kind === "added" || kind === "context")).toBe(true);
    expect(kinds.filter((kind) => kind === "removed")).toHaveLength(1199);
    expect(kinds.filter((kind) => kind === "added")).toHaveLength(1199);
  });

  it("still diffs line by line below the edit limit", () => {
    const before = numbered(1200);
    const after = before.map((line, index) => (index % 100 === 0 ? `${line} changed` : line));

    expect(changed(diffTexts(text(before), text(after)))).toHaveLength(24);
  });

  it("gives up between 990 and 1010 edits, a changed line counting as one removal and one insertion", () => {
    const firstKinds = (changedLines: number) => {
      const before = numbered(1200);
      const after = before.map((line, index) => (index % 2 === 0 && index < changedLines * 2 ? `${line} x` : line));
      return diffTexts(text(before), text(after))
        .slice(0, 3)
        .map((line) => line.kind);
    };

    expect(firstKinds(495)).toEqual(["removed", "added", "context"]);
    expect(firstKinds(505)).toEqual(["removed", "removed", "removed"]);
  });
});

describe("row limit", () => {
  const before = numbered(rowLimit + 10);
  const oneChange = diffTexts(text(before), text([...before.slice(0, 5), "x", ...before.slice(6)]));
  const rewritten = diffTexts(text(numbered(3000)), text(numbered(3000, "new")));

  it("counts the rows each view shows, without collapsed gaps", () => {
    expect(rowCount(oneChange, new Set(), "unified")).toBe(8);
    expect(rowCount(oneChange, new Set(), "split")).toBe(7);
    expect(rowCount(oneChange, "all", "unified")).toBe(rowLimit + 11);
    expect(rowCount(oneChange, "all", "split")).toBe(rowLimit + 10);
    expect(rowCount(rewritten, new Set(), "unified")).toBe(6000);
    expect(rowCount(rewritten, new Set(), "split")).toBe(3000);
  });

  it("decides on the rows of the current view", () => {
    expect(tooManyRows(rewritten, new Set(), "unified")).toBe(true);
    expect(tooManyRows(rewritten, new Set(), "split")).toBe(false);
    expect(tooManyRows(wholeFile(text(numbered(rowLimit)), "added"), "all", "unified")).toBe(false);
    expect(tooManyRows(wholeFile(text(numbered(rowLimit + 1)), "added"), "all", "unified")).toBe(true);
  });

  it("counts the lines of expanded gaps", () => {
    expect(tooManyRows(oneChange, new Set(), "unified")).toBe(false);
    expect(tooManyRows(oneChange, new Set([0]), "unified")).toBe(false);
    expect(tooManyRows(oneChange, new Set([10]), "unified")).toBe(true);
    expect(tooManyRows(oneChange, "all", "split")).toBe(true);
  });

  it("lets through as many rows as were already shown anyway, and no more", () => {
    const shown = rowCount(oneChange, "all", "unified");

    expect(tooManyRows(oneChange, "all", "unified", shown)).toBe(false);
    expect(tooManyRows(oneChange, "all", "unified", shown - 1)).toBe(true);
    expect(tooManyRows(rewritten, new Set(), "unified", 10)).toBe(true);
  });
});

describe("word ranges", () => {
  it("marks the changed words inside a changed line pair", () => {
    const lines = diffTexts("const total = price * count;\n", "const total = price * amount;\n");

    const [removed, added] = changed(lines);
    expect(removed?.old?.words).toEqual([{ start: 22, end: 27 }]);
    expect(added?.new?.words).toEqual([{ start: 22, end: 28 }]);
  });

  it("marks nothing when the pair has too little in common", () => {
    const lines = diffTexts("import { a } from './a';\n", "export default 42;\n");

    expect(changed(lines).map((line) => (line.old ?? line.new)?.words)).toEqual([[], []]);
  });

  it("marks words while a third of the non-space characters are shared, and nothing below that", () => {
    const words = (before: string, after: string) =>
      changed(diffTexts(`${before}\n`, `${after}\n`)).map((line) => (line.old ?? line.new)?.words);

    expect(words("aaa bbbbbb", "aaa cccccc")).toEqual([[{ start: 4, end: 10 }], [{ start: 4, end: 10 }]]);
    expect(words("aaa bbbbbbb", "aaa ccccccc")).toEqual([[], []]);
  });

  it("marks words in lines up to 2000 characters and skips longer ones", () => {
    const words = (length: number) => {
      const stem = "x".repeat(length - 4);
      return changed(diffTexts(`${stem} one\n`, `${stem} two\n`)).map((line) => (line.old ?? line.new)?.words);
    };

    expect(words(2000)).toEqual([[{ start: 1997, end: 2000 }], [{ start: 1997, end: 2000 }]]);
    expect(words(2001)).toEqual([[], []]);
  });

  it("skips the word diff when only one side is over 2000 characters", () => {
    const stem = "x".repeat(1996);
    const words = (before: string, after: string) =>
      changed(diffTexts(`${before}\n`, `${after}\n`)).map((line) => (line.old ?? line.new)?.words);

    expect(words(`${stem} one`, `${stem} two`)).toEqual([[{ start: 1997, end: 2000 }], [{ start: 1997, end: 2000 }]]);
    expect(words(`${stem} one`, `${stem} twoo`)).toEqual([[], []]);
    expect(words(`${stem} twoo`, `${stem} one`)).toEqual([[], []]);
  });

  it("leaves unpaired lines without ranges", () => {
    const lines = diffTexts("a\n", "a\nb\nc\n");

    expect(changed(lines).map((line) => line.new?.words)).toEqual([[], []]);
  });
});

describe("wholeFile", () => {
  it("renders an added file as added lines with new numbers only", () => {
    expect(describeUnified(unifiedRows(wholeFile("x\ny", "added"), "all"))).toEqual(["+.:1 x", "+.:2 y"]);
  });

  it("renders a removed file as removed lines with old numbers only", () => {
    expect(describeUnified(unifiedRows(wholeFile("x\n", "removed"), "all"))).toEqual(["-1:. x"]);
  });

  it("renders an identical file once as context", () => {
    const lines = wholeFile("x\ny\n", "context");

    expect(hasChanges(lines)).toBe(false);
    expect(describeUnified(unifiedRows(lines, "all"))).toEqual([" 1:1 x", " 2:2 y"]);
    expect(describeSplit(splitRows(lines, "all"))).toEqual([" 1 x |  1 x", " 2 y |  2 y"]);
  });
});

describe("languageFor", () => {
  it.each([
    ["src/a.ts", "typescript"],
    ["src/a.tsx", "typescript"],
    ["src/a.mts", "typescript"],
    ["src/a.cts", "typescript"],
    ["a.js", "javascript"],
    ["a.jsx", "javascript"],
    ["a.mjs", "javascript"],
    ["a.cjs", "javascript"],
    ["package.json", "json"],
    ["styles/theme.css", "css"],
    ["styles/theme.SCSS", "scss"],
    ["index.html", "xml"],
    ["index.htm", "xml"],
    ["src/App.vue", "xml"],
    ["README.md", "markdown"],
    [".github/workflows/ci.yml", "yaml"],
    ["config.yaml", "yaml"],
  ])("%s is %s", (path, language) => {
    expect(languageFor(path)).toBe(language);
  });

  it.each(["Makefile", "a.txt", "a.d", ".gitignore", "folder.ts/readme", "a.ts.bak"])("%s is plain text", (path) => {
    expect(languageFor(path)).toBeNull();
  });
});

interface MergeFixture {
  name: string;
  local: string;
  received: string;
  chunks: LineChunk[];
}

const mergeFixtures = import.meta.glob<MergeFixture>("../../../crates/deptide-core/tests/fixtures/merge/*.json", {
  eager: true,
  import: "default",
});

function withoutBom(value: string): string {
  return value.startsWith("﻿") ? value.slice(1) : value;
}

describe("chunksOf", () => {
  it("gives one chunk per run of changed lines, removed lines first, with 0-based starts on both sides", () => {
    const lines = diffTexts("a\nb\nc\nd\ne\n", "a\nB\nc\nd\nE\nf\n");

    expect(chunksOf(lines)).toEqual([
      { oldStart: 1, oldCount: 1, newStart: 1, newCount: 1 },
      { oldStart: 4, oldCount: 1, newStart: 4, newCount: 2 },
    ]);
  });

  it("places a pure insertion and a pure deletion between the lines around them", () => {
    expect(chunksOf(diffTexts("a\nc\n", "a\nb\nc\n"))).toEqual([
      { oldStart: 1, oldCount: 0, newStart: 1, newCount: 1 },
    ]);
    expect(chunksOf(diffTexts("a\nb\nc\n", "a\nc\n"))).toEqual([
      { oldStart: 1, oldCount: 1, newStart: 1, newCount: 0 },
    ]);
  });

  it("gives no chunks for identical texts", () => {
    expect(chunksOf(diffTexts("a\nb\n", "a\nb\n"))).toEqual([]);
  });

  it("names the first line of each run", () => {
    const runs = changeRuns(diffTexts("a\nb\nc\n", "a\nB\nc\nd\n"));

    expect(runs.map((run) => [run.first.kind, run.first.old?.text ?? run.first.new?.text])).toEqual([
      ["removed", "b"],
      ["added", "d"],
    ]);
  });

  it("finds the fixtures shared with the backend", () => {
    expect(Object.keys(mergeFixtures).length).toBeGreaterThanOrEqual(9);
  });

  it.each(Object.entries(mergeFixtures))("matches the chunks of %s", (_path, fixture) => {
    const lines = diffTexts(withoutBom(fixture.local), withoutBom(fixture.received));

    expect(chunksOf(lines), fixture.name).toEqual(fixture.chunks);
  });
});

describe("lineDiff", () => {
  it("returns the lines of diffTexts and says it did not give up", () => {
    const result = lineDiff("a\nb\n", "a\nB\n");

    expect(result.gaveUp).toBe(false);
    expect(result.lines).toEqual(diffTexts("a\nb\n", "a\nB\n"));
  });

  it("says it gave up when the line diff passes the edit limit", () => {
    const before = numbered(1200);
    const after = before.map((line, index) => (index % 2 === 0 ? `${line} changed` : line));

    expect(lineDiff(text(before), text(after)).gaveUp).toBe(true);
  });

  it("does not count a one-sided change as giving up", () => {
    expect(lineDiff("", text(numbered(3000))).gaveUp).toBe(false);
  });
});

describe("chunksOverlapping", () => {
  const exact: LineChunk[] = [
    { oldStart: 1, oldCount: 1, newStart: 1, newCount: 1 },
    { oldStart: 3, oldCount: 0, newStart: 3, newCount: 2 },
    { oldStart: 6, oldCount: 2, newStart: 7, newCount: 0 },
  ];

  it("lists the chunks whose old or new lines meet the block", () => {
    expect(chunksOverlapping({ oldStart: 1, oldCount: 3, newStart: 1, newCount: 1 }, exact)).toEqual([0]);
    expect(chunksOverlapping({ oldStart: 2, oldCount: 0, newStart: 2, newCount: 2 }, exact)).toEqual([1]);
    expect(chunksOverlapping({ oldStart: 0, oldCount: 8, newStart: 0, newCount: 9 }, exact)).toEqual([0, 1, 2]);
    expect(chunksOverlapping({ oldStart: 7, oldCount: 1, newStart: 8, newCount: 0 }, exact)).toEqual([2]);
  });

  it("lists none for a block beside every chunk", () => {
    expect(chunksOverlapping({ oldStart: 2, oldCount: 1, newStart: 2, newCount: 1 }, exact)).toEqual([]);
  });
});
