import { diffArrays, diffWordsWithSpace } from "diff";

export type Language = "typescript" | "javascript" | "json" | "css" | "scss" | "xml" | "markdown" | "yaml";

export type LineEnding = "crlf" | "lf" | "cr" | "none";

export type LineKind = "context" | "added" | "removed";

export interface TextLine {
  text: string;
  ending: LineEnding;
}

export interface WordRange {
  start: number;
  end: number;
}

export interface LineSide {
  number: number;
  text: string;
  ending: LineEnding;
  endingChanged: boolean;
  words: WordRange[];
}

export interface DiffLine {
  kind: LineKind;
  old: LineSide | null;
  new: LineSide | null;
}

export interface Gap {
  kind: "gap";
  id: number;
  count: number;
}

export type UnifiedRow = { kind: "line"; line: DiffLine } | Gap;

export interface SplitCell {
  kind: LineKind;
  side: LineSide;
}

export type SplitRow = { kind: "pair"; left: SplitCell | null; right: SplitCell | null } | Gap;

export type DiffLayout = "unified" | "split";

export interface DiffOptions {
  ignoreWhitespace?: boolean;
}

export const contextLines = 3;

export const rowLimit = 5000;

const maxEditLength = 1000;

const wordDiffMaxLength = 2000;

const languages: Record<string, Language> = {
  ts: "typescript",
  tsx: "typescript",
  mts: "typescript",
  cts: "typescript",
  js: "javascript",
  jsx: "javascript",
  mjs: "javascript",
  cjs: "javascript",
  json: "json",
  css: "css",
  scss: "scss",
  html: "xml",
  htm: "xml",
  vue: "xml",
  md: "markdown",
  yml: "yaml",
  yaml: "yaml",
};

const endings: Record<string, LineEnding> = { "\r\n": "crlf", "\n": "lf", "\r": "cr" };

export function languageFor(path: string): Language | null {
  const name = path.slice(path.lastIndexOf("/") + 1);
  const dot = name.lastIndexOf(".");
  if (dot <= 0) return null;
  return languages[name.slice(dot + 1).toLowerCase()] ?? null;
}

export function splitLines(text: string): TextLine[] {
  const lines: TextLine[] = [];
  const breaks = /\r\n|\n|\r/g;
  let start = 0;

  for (let match = breaks.exec(text); match; match = breaks.exec(text)) {
    lines.push({ text: text.slice(start, match.index), ending: endings[match[0]] ?? "lf" });
    start = match.index + match[0].length;
  }
  if (start < text.length) lines.push({ text: text.slice(start), ending: "none" });
  return lines;
}

function side(line: TextLine, index: number): LineSide {
  return { number: index + 1, text: line.text, ending: line.ending, endingChanged: false, words: [] };
}

export function wholeFile(text: string, kind: LineKind): DiffLine[] {
  return splitLines(text).map((line, index) => ({
    kind,
    old: kind === "added" ? null : side(line, index),
    new: kind === "removed" ? null : side(line, index),
  }));
}

function exactKey(line: TextLine): string {
  return `${line.text}\u0000${line.ending}`;
}

function looseKey(line: TextLine): string {
  return line.text.replace(/\s+/g, "");
}

function sharedEdges(left: readonly string[], right: readonly string[]): { prefix: number; suffix: number } {
  const limit = Math.min(left.length, right.length);
  let prefix = 0;
  while (prefix < limit && left[prefix] === right[prefix]) prefix += 1;
  let suffix = 0;
  while (suffix < limit - prefix && left[left.length - 1 - suffix] === right[right.length - 1 - suffix]) suffix += 1;
  return { prefix, suffix };
}

interface Block {
  equal: boolean;
  oldCount: number;
  newCount: number;
}

function diffBlocks(oldKeys: readonly string[], newKeys: readonly string[]): Block[] {
  const { prefix, suffix } = sharedEdges(oldKeys, newKeys);
  const oldMiddle = oldKeys.slice(prefix, oldKeys.length - suffix);
  const newMiddle = newKeys.slice(prefix, newKeys.length - suffix);
  const blocks: Block[] = [{ equal: true, oldCount: prefix, newCount: prefix }];

  const changes =
    oldMiddle.length && newMiddle.length ? diffArrays(oldMiddle, newMiddle, { maxEditLength }) : undefined;
  if (changes) {
    for (const change of changes) {
      const equal = !change.added && !change.removed;
      blocks.push({
        equal,
        oldCount: change.added ? 0 : change.count,
        newCount: change.removed ? 0 : change.count,
      });
    }
  } else {
    blocks.push({ equal: false, oldCount: oldMiddle.length, newCount: newMiddle.length });
  }

  blocks.push({ equal: true, oldCount: suffix, newCount: suffix });
  return mergeBlocks(blocks);
}

function mergeBlocks(blocks: readonly Block[]): Block[] {
  const merged: Block[] = [];
  for (const block of blocks) {
    if (block.oldCount === 0 && block.newCount === 0) continue;
    const last = merged[merged.length - 1];
    if (last && last.equal === block.equal) {
      last.oldCount += block.oldCount;
      last.newCount += block.newCount;
    } else {
      merged.push({ ...block });
    }
  }
  return merged;
}

function isBlankOnly(lines: readonly TextLine[]): boolean {
  return lines.every((line) => line.text.trim() === "");
}

function toRanges(parts: readonly { start: number; end: number }[]): WordRange[] {
  const ranges: WordRange[] = [];
  for (const part of parts) {
    const last = ranges[ranges.length - 1];
    if (last && last.end === part.start) last.end = part.end;
    else ranges.push({ start: part.start, end: part.end });
  }
  return ranges;
}

function wordRanges(before: string, after: string, ignoreWhitespace: boolean): [WordRange[], WordRange[]] {
  if (before.length > wordDiffMaxLength || after.length > wordDiffMaxLength) return [[], []];

  const oldParts: WordRange[] = [];
  const newParts: WordRange[] = [];
  let oldPosition = 0;
  let newPosition = 0;
  let shared = 0;

  for (const change of diffWordsWithSpace(before, after)) {
    const length = change.value.length;
    const counts = !(ignoreWhitespace && change.value.trim() === "");
    if (change.removed) {
      if (counts) oldParts.push({ start: oldPosition, end: oldPosition + length });
      oldPosition += length;
    } else if (change.added) {
      if (counts) newParts.push({ start: newPosition, end: newPosition + length });
      newPosition += length;
    } else {
      shared += change.value.replace(/\s+/g, "").length;
      oldPosition += length;
      newPosition += length;
    }
  }

  const longest = Math.max(before.replace(/\s+/g, "").length, after.replace(/\s+/g, "").length);
  if (shared * 3 < longest) return [[], []];
  return [toRanges(oldParts), toRanges(newParts)];
}

function pairChanges(removed: LineSide[], added: LineSide[], ignoreWhitespace: boolean): void {
  const pairs = Math.min(removed.length, added.length);
  for (let index = 0; index < pairs; index += 1) {
    const before = removed[index];
    const after = added[index];
    if (!before || !after) continue;
    [before.words, after.words] = wordRanges(before.text, after.text, ignoreWhitespace);
    if (!ignoreWhitespace && before.ending !== after.ending) {
      before.endingChanged = true;
      after.endingChanged = true;
    }
  }
}

export function diffTexts(oldText: string, newText: string, options: DiffOptions = {}): DiffLine[] {
  const ignoreWhitespace = options.ignoreWhitespace ?? false;
  const oldLines = splitLines(oldText);
  const newLines = splitLines(newText);
  const key = ignoreWhitespace ? looseKey : exactKey;
  const lines: DiffLine[] = [];
  let oldIndex = 0;
  let newIndex = 0;

  for (const block of diffBlocks(oldLines.map(key), newLines.map(key))) {
    const removedLines = oldLines.slice(oldIndex, oldIndex + block.oldCount);
    const addedLines = newLines.slice(newIndex, newIndex + block.newCount);

    if (block.equal) {
      removedLines.forEach((line, offset) => {
        const partner = addedLines[offset] ?? line;
        lines.push({ kind: "context", old: side(line, oldIndex + offset), new: side(partner, newIndex + offset) });
      });
    } else if (ignoreWhitespace && isBlankOnly(removedLines) && isBlankOnly(addedLines)) {
      removedLines.forEach((line, offset) =>
        lines.push({ kind: "context", old: side(line, oldIndex + offset), new: null }),
      );
      addedLines.forEach((line, offset) =>
        lines.push({ kind: "context", old: null, new: side(line, newIndex + offset) }),
      );
    } else {
      const removed = removedLines.map((line, offset) => side(line, oldIndex + offset));
      const added = addedLines.map((line, offset) => side(line, newIndex + offset));
      pairChanges(removed, added, ignoreWhitespace);
      for (const entry of removed) lines.push({ kind: "removed", old: entry, new: null });
      for (const entry of added) lines.push({ kind: "added", old: null, new: entry });
    }

    oldIndex += block.oldCount;
    newIndex += block.newCount;
  }

  return lines;
}

export function hasChanges(lines: readonly DiffLine[]): boolean {
  return lines.some((line) => line.kind !== "context");
}

type Visible = { kind: "lines"; lines: DiffLine[] } | Gap;

function visibleParts(lines: readonly DiffLine[], expanded: ReadonlySet<number> | "all"): Visible[] {
  if (expanded === "all") return [{ kind: "lines", lines: [...lines] }];

  const shown = new Array<boolean>(lines.length).fill(false);
  lines.forEach((line, index) => {
    if (line.kind === "context") return;
    const from = Math.max(0, index - contextLines);
    const to = Math.min(lines.length - 1, index + contextLines);
    for (let near = from; near <= to; near += 1) shown[near] = true;
  });

  const parts: Visible[] = [];
  let index = 0;
  while (index < lines.length) {
    const start = index;
    const visible = shown[index] ?? false;
    while (index < lines.length && (shown[index] ?? false) === visible) index += 1;
    const run = lines.slice(start, index);
    if (visible || expanded.has(start)) parts.push({ kind: "lines", lines: run });
    else parts.push({ kind: "gap", id: start, count: run.length });
  }
  return mergeVisible(parts);
}

function mergeVisible(parts: readonly Visible[]): Visible[] {
  const merged: Visible[] = [];
  for (const part of parts) {
    const last = merged[merged.length - 1];
    if (part.kind === "lines" && last?.kind === "lines") last.lines.push(...part.lines);
    else merged.push(part);
  }
  return merged;
}

export function rowCount(
  lines: readonly DiffLine[],
  expanded: ReadonlySet<number> | "all",
  layout: DiffLayout,
): number {
  const rows = layout === "split" ? splitRows(lines, expanded) : unifiedRows(lines, expanded);
  return rows.filter((row) => row.kind !== "gap").length;
}

/** True when the view would show more than `rowLimit` rows and more than `accepted`, the count already shown anyway. */
export function tooManyRows(
  lines: readonly DiffLine[],
  expanded: ReadonlySet<number> | "all",
  layout: DiffLayout,
  accepted = 0,
): boolean {
  return rowCount(lines, expanded, layout) > Math.max(rowLimit, accepted);
}

export function unifiedRows(lines: readonly DiffLine[], expanded: ReadonlySet<number> | "all"): UnifiedRow[] {
  return visibleParts(lines, expanded).flatMap((part): UnifiedRow[] =>
    part.kind === "gap" ? [part] : part.lines.map((line) => ({ kind: "line", line })),
  );
}

function cell(kind: LineKind, value: LineSide | null): SplitCell | null {
  return value ? { kind, side: value } : null;
}

function pairLines(lines: readonly DiffLine[]): SplitRow[] {
  const rows: SplitRow[] = [];
  let index = 0;

  while (index < lines.length) {
    const line = lines[index];
    if (line?.kind === "context" && line.old && line.new) {
      rows.push({ kind: "pair", left: cell("context", line.old), right: cell("context", line.new) });
      index += 1;
      continue;
    }

    const left: SplitCell[] = [];
    const right: SplitCell[] = [];
    while (index < lines.length) {
      const current = lines[index];
      if (!current || (current.kind === "context" && current.old && current.new)) break;
      const oldCell = cell(current.kind, current.old);
      const newCell = cell(current.kind, current.new);
      if (oldCell) left.push(oldCell);
      if (newCell) right.push(newCell);
      index += 1;
    }
    for (let row = 0; row < Math.max(left.length, right.length); row += 1) {
      rows.push({ kind: "pair", left: left[row] ?? null, right: right[row] ?? null });
    }
  }

  return rows;
}

export function splitRows(lines: readonly DiffLine[], expanded: ReadonlySet<number> | "all"): SplitRow[] {
  return visibleParts(lines, expanded).flatMap((part): SplitRow[] =>
    part.kind === "gap" ? [part] : pairLines(part.lines),
  );
}
