import type { ProjectGroup, ProjectView } from "@/api/types";

export function groupSelection(group: ProjectGroup, available: readonly ProjectView[]): string[] {
  const selectable = new Set(available.filter((project) => project.exists).map((project) => project.name));
  return group.projects.filter((name) => selectable.has(name));
}

export function addGroupToSelection(
  group: ProjectGroup,
  available: readonly ProjectView[],
  current: readonly string[],
): string[] {
  const added = groupSelection(group, available).filter((name) => !current.includes(name));
  return [...current, ...added];
}

export function withGroup(groups: readonly ProjectGroup[], name: string, projects: readonly string[]): ProjectGroup[] {
  const saved: ProjectGroup = { name: name.trim(), projects: [...projects] };
  if (!groups.some((group) => group.name === saved.name)) return [...groups, saved];
  return groups.map((group) => (group.name === saved.name ? saved : group));
}

export function renameGroup(groups: readonly ProjectGroup[], from: string, to: string): ProjectGroup[] {
  return groups.map((group) => (group.name === from ? { ...group, name: to.trim() } : group));
}

export function withoutGroup(groups: readonly ProjectGroup[], name: string): ProjectGroup[] {
  return groups.filter((group) => group.name !== name);
}
