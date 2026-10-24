import type { DependencyDrift, DriftCell, LibraryDrift } from "@/api/types";

export interface DriftFilter {
  library: string;
  project: string;
  onlyMismatches: boolean;
}

export interface DriftView {
  projects: string[];
  libraries: LibraryDrift[];
}

function includesText(value: string, text: string): boolean {
  return value.toLowerCase().includes(text.trim().toLowerCase());
}

export function isMismatch(cell: DriftCell): boolean {
  return cell.outOfRange || cell.differs;
}

export function filterDrift(drift: DependencyDrift, filter: DriftFilter): DriftView {
  const projects = drift.projects.filter((project) => includesText(project, filter.project));
  const visible = new Set(projects);

  const libraries = drift.libraries.filter((library) => {
    if (!includesText(library.name, filter.library)) return false;
    const cells = library.cells.filter((cell) => visible.has(cell.project));
    if (cells.length === 0) return false;
    return !filter.onlyMismatches || cells.some(isMismatch);
  });

  return { projects, libraries };
}

export type VersionPickKind = "newest" | "latest" | "local";

export interface VersionPick {
  kind: VersionPickKind;
  version: string;
}

export function versionPicks(library: LibraryDrift): VersionPick[] {
  const candidates: [VersionPickKind, string | null][] = [
    ["newest", library.newestInstalled],
    ["latest", library.latest],
    ["local", library.localVersion],
  ];
  const picks: VersionPick[] = [];

  for (const [kind, version] of candidates) {
    if (version && !picks.some((pick) => pick.version === version)) picks.push({ kind, version });
  }
  return picks;
}

export interface DriftSelection {
  library: string;
  projects: string[];
}

export function toggleCell(selection: DriftSelection | null, library: string, project: string): DriftSelection | null {
  if (selection?.library !== library) return { library, projects: [project] };

  const projects = selection.projects.includes(project)
    ? selection.projects.filter((name) => name !== project)
    : [...selection.projects, project];
  return projects.length > 0 ? { library, projects } : null;
}

export function selectRow(library: LibraryDrift, visibleProjects: readonly string[]): DriftSelection {
  const declaring = new Set(library.cells.map((cell) => cell.project));
  return { library: library.name, projects: visibleProjects.filter((project) => declaring.has(project)) };
}
