import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { DependencyDrift, DependencyGraph, ProjectCopies } from "@/api/types";
import { handleMockCommand } from "@/dev/mock/handlers";
import { fakeRoot, workspaceState } from "@/dev/mock/workspace";

async function answer<T>(command: string, args: Record<string, unknown>): Promise<T> {
  const pending = handleMockCommand(command, args);
  await vi.runAllTimersAsync();
  return (await pending) as T;
}

describe("mock dependencies backend", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("answers drift for configured projects and adds latest versions only when asked", async () => {
    const configured = new Set(workspaceState.config.projects.map((project) => project.name));

    const plain = await answer<DependencyDrift>("read_dependency_drift", { root: fakeRoot, checkLatest: false });
    const checked = await answer<DependencyDrift>("read_dependency_drift", { root: fakeRoot, checkLatest: true });

    expect(plain.libraries.length).toBeGreaterThan(0);
    for (const library of plain.libraries) {
      expect(library.latest).toBeNull();
      for (const cell of library.cells) expect(configured.has(cell.project)).toBe(true);
    }
    expect(checked.libraries.some((library) => library.latest !== null)).toBe(true);
    expect(checked.libraries.some((library) => library.latestError !== null)).toBe(true);
    expect(checked.libraries.flatMap((library) => library.cells).some((cell) => cell.behind)).toBe(true);
  });

  it("answers a graph whose edges join configured projects and copies per scope", async () => {
    const configured = new Set(workspaceState.config.projects.map((project) => project.name));

    const graph = await answer<DependencyGraph>("read_dependency_graph", { root: fakeRoot });
    const libraries = await answer<ProjectCopies[]>("find_installed_copies", { root: fakeRoot, scope: "libraries" });
    const all = await answer<ProjectCopies[]>("find_installed_copies", { root: fakeRoot, scope: "all-packages" });

    expect(graph.edges.length).toBeGreaterThan(0);
    for (const edge of graph.edges) {
      expect(configured.has(edge.consumer)).toBe(true);
      expect(configured.has(edge.library)).toBe(true);
    }
    const count = (copies: ProjectCopies[]) => copies.flatMap((project) => project.packages).length;
    expect(count(libraries)).toBeGreaterThan(0);
    expect(count(all)).toBeGreaterThan(count(libraries));
  });
});
