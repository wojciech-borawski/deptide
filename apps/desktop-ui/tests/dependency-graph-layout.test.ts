import { describe, expect, it } from "vitest";

import type { DependencyGraph } from "@/api/types";
import { layoutGraph, type PlacedNode } from "@/lib/dependency-graph-layout";

const graph: DependencyGraph = {
  nodes: [
    { name: "core", packageName: "@demo/core", layer: 0, dependsOn: [] },
    { name: "docs", packageName: "docs", layer: 0, dependsOn: [] },
    { name: "ui", packageName: "@demo/ui", layer: 1, dependsOn: ["core"] },
    { name: "admin", packageName: "admin", layer: 1, dependsOn: ["core"] },
    { name: "web", packageName: "web", layer: 2, dependsOn: ["ui"] },
  ],
  edges: [
    { consumer: "ui", library: "core", section: "dependencies" },
    { consumer: "admin", library: "core", section: "peer" },
    { consumer: "web", library: "ui", section: "dependencies" },
  ],
  cycles: [],
};

function find(nodes: PlacedNode[], name: string): PlacedNode {
  const node = nodes.find((entry) => entry.name === name);
  if (!node) throw new Error(`${name} is not placed`);
  return node;
}

describe("graph layout", () => {
  it("puts libraries left and consumers right, one column per layer, inside the given width", () => {
    const layout = layoutGraph(graph, 760);

    expect(layout.nodes.map((node) => node.name)).toEqual(["core", "ui", "admin", "web"]);
    expect(layout.unlinked).toEqual(["docs"]);
    const [core, ui, admin, web] = ["core", "ui", "admin", "web"].map((name) => find(layout.nodes, name));
    if (!core || !ui || !admin || !web) throw new Error("not placed");
    expect(core.x).toBeLessThan(ui.x);
    expect(ui.x).toBe(admin.x);
    expect(ui.y).not.toBe(admin.y);
    expect(ui.x).toBeLessThan(web.x);
    for (const node of layout.nodes) {
      expect(node.x).toBeGreaterThanOrEqual(0);
      expect(node.x + node.width).toBeLessThanOrEqual(760);
      expect(node.y + node.height).toBeLessThanOrEqual(layout.height);
    }
  });

  it("draws each edge from the library's right side to the consumer's left side", () => {
    const layout = layoutGraph(graph, 760);
    const edge = layout.edges.find((entry) => entry.consumer === "web");
    if (!edge) throw new Error("no edge");
    const ui = find(layout.nodes, "ui");
    const web = find(layout.nodes, "web");

    const numbers = edge.path.match(/-?\d+(\.\d+)?/g)?.map(Number) ?? [];
    expect(numbers.slice(0, 2)).toEqual([ui.x + ui.width, ui.y + ui.height / 2]);
    const [endX = 0, endY = 0] = numbers.slice(-2);
    expect(endX).toBe(web.x);
    expect(endY).toBeGreaterThan(web.y);
    expect(endY).toBeLessThan(web.y + web.height);
    expect(layout.edges.map((entry) => entry.section)).toEqual(["dependencies", "peer", "dependencies"]);
  });
});
