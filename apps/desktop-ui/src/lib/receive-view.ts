import type { FileStatus, ReceiveFile, ReceiveProjectPlan, ReceiveSelection } from "@/api/types";

export type StatusCounts = Record<FileStatus, number>;

export type SelectionMode = "changes" | "all" | "none";

export type CheckState = "checked" | "unchecked" | "indeterminate";

export interface TreeFolder {
  kind: "folder";
  name: string;
  path: string;
  children: TreeNode[];
  counts: StatusCounts;
  files: string[];
}

export interface TreeFile {
  kind: "file";
  name: string;
  path: string;
  file: ReceiveFile;
}

export type TreeNode = TreeFolder | TreeFile;

export interface TreeRow {
  node: TreeNode;
  depth: number;
}

export interface FileGroup {
  status: FileStatus;
  files: ReceiveFile[];
}

export interface RemovalGroup {
  source: string;
  target: string;
  files: string[];
}

export const statusTones: Readonly<Record<FileStatus, string>> = {
  added: "badge-ok",
  replaced: "badge-warn",
  removed: "badge-failed",
  whitespace: "badge-skipped",
  identical: "",
};

export const changeOrder: readonly FileStatus[] = ["replaced", "whitespace", "added", "removed", "identical"];

const defaultStatuses: ReadonlySet<FileStatus> = new Set(["added", "replaced", "removed"]);

function isAffected(file: ReceiveFile): boolean {
  return file.status !== "identical";
}

function compareNames(left: string, right: string): number {
  if (left === right) return 0;
  return left < right ? -1 : 1;
}

function emptyCounts(): StatusCounts {
  return { added: 0, replaced: 0, whitespace: 0, identical: 0, removed: 0 };
}

export function hasChanges(project: ReceiveProjectPlan): boolean {
  return project.files.some(isAffected);
}

export function splitByChanges(projects: readonly ReceiveProjectPlan[]): {
  changed: ReceiveProjectPlan[];
  unchanged: ReceiveProjectPlan[];
} {
  return {
    changed: projects.filter(hasChanges),
    unchanged: projects.filter((project) => !hasChanges(project)),
  };
}

export function selectionFor(files: readonly ReceiveFile[], mode: SelectionMode): string[] {
  if (mode === "none") return [];
  const ticked = mode === "all" ? files : files.filter((file) => defaultStatuses.has(file.status));
  return ticked.map((file) => file.relative);
}

export function carrySelection(
  previous: ReceiveProjectPlan | undefined,
  next: ReceiveProjectPlan,
  selected: ReadonlySet<string>,
): string[] {
  const before = new Map(previous?.files.map((file) => [file.relative, file.status]));
  return next.files
    .filter((file) =>
      before.get(file.relative) === file.status ? selected.has(file.relative) : defaultStatuses.has(file.status),
    )
    .map((file) => file.relative);
}

export function nextTabIndex(current: number, key: string, count: number): number | null {
  if (count === 0) return null;
  if (key === "ArrowRight") return (current + 1) % count;
  if (key === "ArrowLeft") return (current - 1 + count) % count;
  return null;
}

export function visibleFiles(files: readonly ReceiveFile[], onlyAffected: boolean): ReceiveFile[] {
  return onlyAffected ? files.filter(isAffected) : [...files];
}

export function comparePaths(left: string, right: string): number {
  const leftParts = left.split("/");
  const rightParts = right.split("/");
  const shared = Math.min(leftParts.length, rightParts.length);

  for (let index = 0; index < shared; index += 1) {
    const order = compareNames(leftParts[index] ?? "", rightParts[index] ?? "");
    if (order !== 0) return order;
  }
  return leftParts.length - rightParts.length;
}

export function groupByChange(files: readonly ReceiveFile[]): FileGroup[] {
  const sorted = [...files].sort((left, right) => comparePaths(left.relative, right.relative));
  return changeOrder
    .map((status) => ({ status, files: sorted.filter((file) => file.status === status) }))
    .filter((group) => group.files.length > 0);
}

function sortNodes(nodes: TreeNode[]): void {
  nodes.sort((left, right) => {
    if (left.kind !== right.kind) return left.kind === "folder" ? -1 : 1;
    return compareNames(left.name, right.name);
  });
  for (const node of nodes) if (node.kind === "folder") sortNodes(node.children);
}

export function buildTree(files: readonly ReceiveFile[]): TreeNode[] {
  const roots: TreeNode[] = [];
  const folders = new Map<string, TreeFolder>();
  const sorted = [...files].sort((left, right) => comparePaths(left.relative, right.relative));

  for (const file of sorted) {
    const parts = file.relative.split("/");
    const name = parts.pop() ?? file.relative;
    let siblings = roots;
    let path = "";

    for (const part of parts) {
      path = path ? `${path}/${part}` : part;
      let folder = folders.get(path);
      if (!folder) {
        folder = { kind: "folder", name: part, path, children: [], counts: emptyCounts(), files: [] };
        folders.set(path, folder);
        siblings.push(folder);
      }
      folder.counts[file.status] += 1;
      folder.files.push(file.relative);
      siblings = folder.children;
    }

    siblings.push({ kind: "file", name, path: file.relative, file });
  }

  sortNodes(roots);
  return roots;
}

export function checkState(paths: readonly string[], selected: ReadonlySet<string>): CheckState {
  const ticked = paths.filter((path) => selected.has(path)).length;
  if (ticked === 0) return "unchecked";
  return ticked === paths.length ? "checked" : "indeterminate";
}

export function defaultExpanded(files: readonly ReceiveFile[]): Set<string> {
  const expanded = new Set<string>();
  for (const file of files.filter(isAffected)) {
    const parts = file.relative.split("/").slice(0, -1);
    for (let depth = 1; depth <= parts.length; depth += 1) expanded.add(parts.slice(0, depth).join("/"));
  }
  return expanded;
}

export function flattenTree(nodes: readonly TreeNode[], isExpanded: (path: string) => boolean): TreeRow[] {
  const rows: TreeRow[] = [];

  function visit(level: readonly TreeNode[], depth: number): void {
    for (const node of level) {
      rows.push({ node, depth });
      if (node.kind === "folder" && isExpanded(node.path)) visit(node.children, depth + 1);
    }
  }

  visit(nodes, 0);
  return rows;
}

export function toSelection(project: ReceiveProjectPlan, selected: ReadonlySet<string>): ReceiveSelection {
  const ticked = project.files.filter((file) => selected.has(file.relative));
  return {
    source: project.source,
    target: project.target,
    files: ticked.filter((file) => file.status !== "removed").map((file) => file.relative),
    delete: ticked.filter((file) => file.status === "removed").map((file) => file.relative),
  };
}

export function selectedRemovals(
  projects: readonly ReceiveProjectPlan[],
  selectedFor: (source: string) => ReadonlySet<string>,
): RemovalGroup[] {
  return projects
    .map((project) => ({
      source: project.source,
      target: project.target,
      files: toSelection(project, selectedFor(project.source)).delete,
    }))
    .filter((group) => group.files.length > 0);
}

export function stepThrough(order: readonly string[], current: string | null, step: 1 | -1): string | null {
  const index = current === null ? -1 : order.indexOf(current);
  if (index === -1) return (step === 1 ? order[0] : order[order.length - 1]) ?? null;
  return order[Math.min(order.length - 1, Math.max(0, index + step))] ?? null;
}

export interface FileCursor {
  cursor: string | null;
  preview: string | null;
}

export interface CursorMove extends FileCursor {
  /** The folder to expand or collapse. */
  toggle?: string;
}

/** Applies a key to the cursor; `folders` maps each visible tree folder to whether it is expanded. */
export function fileListKey(
  key: string,
  order: readonly string[],
  state: FileCursor,
  folders: ReadonlyMap<string, boolean> = new Map(),
): CursorMove | null {
  const visible = state.cursor !== null && order.includes(state.cursor) ? state.cursor : null;
  const moveTo = (next: string): CursorMove => ({
    cursor: next,
    preview: state.preview === null || folders.has(next) ? state.preview : next,
  });

  if (key === "ArrowDown" || key === "ArrowUp") {
    const next = stepThrough(order, visible, key === "ArrowDown" ? 1 : -1);
    return next === null ? null : moveTo(next);
  }
  if (visible !== null && folders.has(visible)) {
    const expanded = folders.get(visible) ?? false;
    if (key === "Enter" || (key === "ArrowRight" && !expanded) || (key === "ArrowLeft" && expanded)) {
      return { cursor: visible, preview: state.preview, toggle: visible };
    }
    if (key === "ArrowRight") {
      const child = order[order.indexOf(visible) + 1];
      return child?.startsWith(`${visible}/`) ? moveTo(child) : null;
    }
  }
  if (key === "ArrowLeft" && visible !== null) {
    const parent = visible.slice(0, Math.max(0, visible.lastIndexOf("/")));
    return folders.has(parent) ? moveTo(parent) : null;
  }
  if (key === "Enter" && visible !== null) return { cursor: visible, preview: visible };
  if (key === "Escape" && state.preview !== null) return { cursor: state.cursor, preview: null };
  return null;
}
