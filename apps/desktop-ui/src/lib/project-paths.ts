export type PathSeparator = "\\" | "/";

export interface SplitPath {
  separator: PathSeparator;
  segments: string[];
}

export interface GroupedItem<T> {
  item: T;
  rest: string[];
}

export interface ProjectGroup<T> {
  folder: string | null;
  items: GroupedItem<T>[];
}

export interface GroupedProjects<T> {
  separator: PathSeparator;
  prefix: string[];
  groups: ProjectGroup<T>[];
}

export function splitPath(path: string): SplitPath {
  const separator: PathSeparator = path.includes("\\") ? "\\" : "/";
  const segments = path.split(/[\\/]/).filter((segment, index) => segment.length > 0 || index === 0);
  return { separator, segments };
}

export function joinPath(segments: string[], separator: PathSeparator): string {
  return segments.join(separator);
}

export function commonPrefixLength(paths: string[][]): number {
  if (!paths.length) return 0;
  const limit = Math.min(...paths.map((segments) => segments.length)) - 1;
  const first = paths[0] ?? [];
  let length = 0;

  while (length < limit && paths.every((segments) => segments[length] === first[length])) {
    length += 1;
  }

  return length;
}

export function groupByFolder<T>(items: T[], pathOf: (item: T) => string): GroupedProjects<T> {
  const split = items.map((item) => ({ item, ...splitPath(pathOf(item)) }));
  const separator = split[0]?.separator ?? "\\";
  const prefixLength = commonPrefixLength(split.map((entry) => entry.segments));
  const prefix = split[0]?.segments.slice(0, prefixLength) ?? [];
  const groups: ProjectGroup<T>[] = [];

  for (const entry of split) {
    const rest = entry.segments.slice(prefixLength);
    const folderSegments = rest.slice(0, -1);
    const folder = folderSegments.length ? joinPath(folderSegments, entry.separator) : null;
    const existing = groups.find((group) => group.folder === folder);

    if (existing) existing.items.push({ item: entry.item, rest });
    else groups.push({ folder, items: [{ item: entry.item, rest }] });
  }

  return { separator, prefix, groups };
}
