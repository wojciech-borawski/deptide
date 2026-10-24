import { afterEach, describe, expect, it, vi } from "vitest";

import type { DependencyGraph } from "@/api/types";
import { mockDependencyGraph } from "@/dev/mock/dependencies";
import { ARROW_SIZE, layoutGraph, type GraphLayout, type PlacedNode } from "@/lib/dependency-graph-layout";

type Point = [number, number];
type Segment = { from: Point; to: Point; edge: string; source: string };

const BOX_CLEARANCE = 16;
const ROUTE_CLEARANCE = 12;

function pointsAlongPath(path: string): Point[] {
  const tokens = path.match(/[MLHVC]|-?\d+(?:\.\d+)?/g) ?? [];
  const points: Point[] = [];
  let command = "";
  let index = 0;
  const next = (): number => Number(tokens[index++]);
  while (index < tokens.length) {
    if (/[MLHVC]/.test(tokens[index]!)) command = tokens[index++]!;
    const last = points.at(-1) ?? [0, 0];
    if (command === "M" || command === "L") points.push([next(), next()]);
    else if (command === "H") points.push([next(), last[1]]);
    else if (command === "V") points.push([last[0], next()]);
    else if (command === "C") {
      const c1: Point = [next(), next()];
      const c2: Point = [next(), next()];
      const end: Point = [next(), next()];
      for (let step = 1; step <= 40; step++) {
        const t = step / 40;
        const u = 1 - t;
        points.push([
          u * u * u * last[0] + 3 * u * u * t * c1[0] + 3 * u * t * t * c2[0] + t * t * t * end[0],
          u * u * u * last[1] + 3 * u * u * t * c1[1] + 3 * u * t * t * c2[1] + t * t * t * end[1],
        ]);
      }
    } else throw new Error(`unexpected path command in ${path}`);
  }
  return points;
}

function segmentsOf(layout: GraphLayout): Segment[] {
  return layout.edges.flatMap((edge) => {
    const points = pointsAlongPath(edge.path);
    return points.slice(1).map((to, index) => ({
      from: points[index]!,
      to,
      edge: `${edge.library}->${edge.consumer}`,
      source: edge.library,
    }));
  });
}

function samples(segment: Segment): Point[] {
  return Array.from({ length: 201 }, (_, step): Point => {
    const t = step / 200;
    return [
      segment.from[0] + (segment.to[0] - segment.from[0]) * t,
      segment.from[1] + (segment.to[1] - segment.from[1]) * t,
    ];
  });
}

function distanceToBox(segment: Segment, box: PlacedNode): number {
  return Math.min(
    ...samples(segment).map(([x, y]) =>
      Math.hypot(Math.max(box.x - x, 0, x - (box.x + box.width)), Math.max(box.y - y, 0, y - (box.y + box.height))),
    ),
  );
}

function insideInterior(segment: Segment, box: PlacedNode): boolean {
  return samples(segment).some(
    ([x, y]) => x > box.x + 0.5 && x < box.x + box.width - 0.5 && y > box.y + 0.5 && y < box.y + box.height - 0.5,
  );
}

function onBorder([x, y]: Point, box: PlacedNode): boolean {
  const withinY = y >= box.y && y <= box.y + box.height;
  const withinX = x >= box.x && x <= box.x + box.width;
  return (
    (withinY && (x === box.x || x === box.x + box.width)) || (withinX && (y === box.y || y === box.y + box.height))
  );
}

function gapBetweenOverlappingParallels(a: Segment, b: Segment): number | null {
  const horizontal = (s: Segment) => s.from[1] === s.to[1];
  const overlap = (a1: number, a2: number, b1: number, b2: number) =>
    Math.min(Math.max(a1, a2), Math.max(b1, b2)) - Math.max(Math.min(a1, a2), Math.min(b1, b2)) > 0.5;
  if (horizontal(a) !== horizontal(b)) return null;
  const along = horizontal(a) ? 0 : 1;
  const across = horizontal(a) ? 1 : 0;
  if (!overlap(a.from[along], a.to[along], b.from[along], b.to[along])) return null;
  return Math.abs(a.from[across] - b.from[across]);
}

function arrowheadBox(end: Point): PlacedNode {
  return {
    name: "arrowhead",
    layer: 0,
    x: end[0] - ARROW_SIZE,
    y: end[1] - ARROW_SIZE / 2,
    width: ARROW_SIZE,
    height: ARROW_SIZE,
  };
}

function problems(layout: GraphLayout): string[] {
  const found: string[] = [];
  const segments = segmentsOf(layout);
  const box = (name: string) => layout.nodes.find((node) => node.name === name)!;
  if (!(ARROW_SIZE > 0)) found.push("the layout does not say how large an arrowhead is");

  for (const segment of segments) {
    for (const node of layout.nodes) {
      if (insideInterior(segment, node)) found.push(`${segment.edge} crosses ${node.name}`);
      else if (
        !onBorder(segment.from, node) &&
        !onBorder(segment.to, node) &&
        distanceToBox(segment, node) < BOX_CLEARANCE
      )
        found.push(`${segment.edge} passes ${node.name} closer than ${BOX_CLEARANCE}`);
    }
  }

  for (const [index, a] of segments.entries()) {
    for (const b of segments.slice(index + 1)) {
      const gap = a.source === b.source ? null : gapBetweenOverlappingParallels(a, b);
      if (gap !== null && gap < ROUTE_CLEARANCE) found.push(`${a.edge} runs ${gap} from ${b.edge}`);
    }
  }

  const heads: { name: string; area: PlacedNode }[] = [];
  for (const edge of layout.edges) {
    const name = `${edge.library}->${edge.consumer}`;
    const points = pointsAlongPath(edge.path);
    const [start, firstBend, end, beforeEnd] = [points[0]!, points[1]!, points.at(-1)!, points.at(-2)!];
    const library = box(edge.library);
    const consumer = box(edge.consumer);
    if (start[0] !== library.x + library.width || start[1] !== library.y + library.height / 2)
      found.push(`${name} does not leave the middle of the library's right side`);
    if (end[0] !== consumer.x || end[1] <= consumer.y || end[1] >= consumer.y + consumer.height)
      found.push(`${name} does not end on the consumer's left side`);
    if (beforeEnd[1] !== end[1] || beforeEnd[0] >= end[0]) found.push(`${name} does not enter its target straight`);
    if (firstBend[0] - start[0] < BOX_CLEARANCE)
      found.push(`${name} turns ${firstBend[0] - start[0]} after the library`);
    if (end[0] - beforeEnd[0] < BOX_CLEARANCE) found.push(`${name} turns ${end[0] - beforeEnd[0]} before the consumer`);
    const area = arrowheadBox(end);
    if (end[1] - ARROW_SIZE / 2 < consumer.y || end[1] + ARROW_SIZE / 2 > consumer.y + consumer.height)
      found.push(`${name} has an arrowhead reaching past the consumer's side`);
    for (const other of heads) {
      const apart =
        area.x >= other.area.x + other.area.width ||
        other.area.x >= area.x + area.width ||
        area.y >= other.area.y + other.area.height ||
        other.area.y >= area.y + area.height;
      if (!apart) found.push(`${name} has an arrowhead overlapping the one of ${other.name}`);
    }
    for (const segment of segments) {
      if (segment.edge !== name && distanceToBox(segment, area) === 0)
        found.push(`${segment.edge} touches the arrowhead of ${name}`);
    }
    heads.push({ name, area });
  }

  const trunks = new Map<string, Set<number>>();
  for (const edge of layout.edges) {
    const points = pointsAlongPath(edge.path);
    const turn = points.findIndex((point, index) => index > 0 && point[0] === points[index - 1]![0]);
    if (turn > 0 && points[turn]![1] !== points[turn - 1]![1]) {
      const xs = trunks.get(edge.library) ?? new Set<number>();
      xs.add(points[turn]![0]);
      trunks.set(edge.library, xs);
    }
  }
  for (const [library, xs] of trunks) if (xs.size > 1) found.push(`${library} turns its arrows at ${xs.size} lanes`);

  return found;
}

const small: DependencyGraph = {
  nodes: [
    { name: "core", packageName: "@demo/core", layer: 0, dependsOn: [] },
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

function fourDenselyLinkedLayers(): DependencyGraph {
  const layers = [4, 3, 4, 2];
  const nodes = layers.flatMap((count, layer) =>
    Array.from({ length: count }, (_, row) => ({
      name: `n${layer}${row}`,
      packageName: `n${layer}${row}`,
      layer,
      dependsOn: [] as string[],
    })),
  );
  const edges: DependencyGraph["edges"] = [];
  for (const consumer of nodes) {
    for (const library of nodes) {
      const linked =
        (consumer.name.charCodeAt(1) * 7 + consumer.name.charCodeAt(2) * 3 + library.name.charCodeAt(2)) % 3;
      if (consumer.layer > library.layer && linked !== 0)
        edges.push({ consumer: consumer.name, library: library.name, section: "dependencies" });
    }
  }
  edges.push({ consumer: "n01", library: "n21", section: "dev" });
  edges.push({ consumer: "n11", library: "n12", section: "peer" });
  return { nodes, edges, cycles: [["n01", "n21"]] };
}

const sixLibrariesInOneColumnForOneApp: DependencyGraph = {
  nodes: [
    ...Array.from({ length: 6 }, (_, row) => ({
      name: `lib${row}`,
      packageName: `lib${row}`,
      layer: 0,
      dependsOn: [],
    })),
    { name: "app", packageName: "app", layer: 1, dependsOn: [] },
  ],
  edges: Array.from({ length: 6 }, (_, row) => ({
    consumer: "app",
    library: `lib${row}`,
    section: "dependencies" as const,
  })),
  cycles: [],
};

describe("graph edge routing", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("keeps every edge of the mock graph clear of boxes, apart from other boxes' edges and on its own arrowhead", () => {
    expect(problems(layoutGraph(mockDependencyGraph(), 480))).toEqual([]);
    expect(problems(layoutGraph(mockDependencyGraph(), 760))).toEqual([]);
  });

  it("routes the back edge of the mock cycle around the boxes", () => {
    vi.stubGlobal("location", { search: "?deps=cycle" });
    const layout = layoutGraph(mockDependencyGraph(), 760);
    const narrow = layoutGraph(mockDependencyGraph(), 480);

    expect(layout.edges.filter((edge) => edge.back).map((edge) => `${edge.library}->${edge.consumer}`)).toEqual([
      "Billing-Api->Orders-Api",
    ]);
    expect([...problems(layout), ...problems(narrow)]).toEqual([]);
  });

  it("holds for a small graph, a dense graph with long, back and same-layer edges, and six inputs at several widths", () => {
    const found = [small, fourDenselyLinkedLayers(), sixLibrariesInOneColumnForOneApp].flatMap((graph) =>
      [480, 760, 1400].flatMap((width) => problems(layoutGraph(graph, width)).map((entry) => `${width}: ${entry}`)),
    );

    expect(found).toEqual([]);
  });

  it("draws the line from the library beside a project in its row straight into the project's middle", () => {
    const layout = layoutGraph(small, 760);
    const coreToUi = layout.edges.find((edge) => edge.library === "core" && edge.consumer === "ui")!;
    const ui = layout.nodes.find((node) => node.name === "ui")!;

    expect(new Set(coreToUi.points.map(([, y]) => y))).toEqual(new Set([ui.y + ui.height / 2]));
    expect(coreToUi.points.at(-1)![0]).toBe(ui.x);
    expect(ui.height).toBe(30);
  });

  it("gives the library beside a project with several inputs the middle port and stacks the others in arrival order", () => {
    const portsOf = (layout: GraphLayout, consumer: string, libraries: string[]) => {
      const box = layout.nodes.find((node) => node.name === consumer)!;
      const middle = box.y + box.height / 2;
      return libraries.map((library) => {
        const edge = layout.edges.find(
          (candidate) => candidate.library === library && candidate.consumer === consumer,
        )!;
        return [library, edge.points.at(-1)![1] - middle];
      });
    };
    const mock = layoutGraph(mockDependencyGraph(), 760);
    const shop = mock.nodes.find((node) => node.name === "Shop-Frontend")!;
    const kitToShop = mock.edges.find((edge) => edge.library === "Demo-UiKit" && edge.consumer === "Shop-Frontend")!;
    const threeInputs = layoutGraph(
      {
        nodes: [
          { name: "core", packageName: "core", layer: 0, dependsOn: [] },
          { name: "kit", packageName: "kit", layer: 1, dependsOn: [] },
          { name: "extra", packageName: "extra", layer: 1, dependsOn: [] },
          { name: "app", packageName: "app", layer: 2, dependsOn: [] },
        ],
        edges: [
          { consumer: "app", library: "extra", section: "dependencies" },
          { consumer: "app", library: "kit", section: "dependencies" },
          { consumer: "app", library: "core", section: "dependencies" },
        ],
        cycles: [],
      },
      760,
    );

    expect(new Set(kitToShop.points.map(([, y]) => y))).toEqual(new Set([shop.y + shop.height / 2]));
    expect(portsOf(mock, "Shop-Frontend", ["Demo-Core", "Demo-UiKit"])).toEqual([
      ["Demo-Core", -12],
      ["Demo-UiKit", 0],
    ]);
    expect(portsOf(threeInputs, "app", ["core", "kit", "extra"])).toEqual([
      ["core", -12],
      ["kit", 0],
      ["extra", 12],
    ]);
  });

  it("keeps six inputs around the middle when the library beside the project is its first input", () => {
    const app = layoutGraph(sixLibrariesInOneColumnForOneApp, 760).nodes.find((node) => node.name === "app")!;

    expect(app.height).toBe(88);
  });

  it("draws only the project with many inputs taller", () => {
    const layout = layoutGraph(sixLibrariesInOneColumnForOneApp, 760);

    expect(layout.nodes.map((node) => [node.name, node.height])).toEqual([
      ["lib0", 30],
      ["lib1", 30],
      ["lib2", 30],
      ["lib3", 30],
      ["lib4", 30],
      ["lib5", 30],
      ["app", 88],
    ]);
  });
});
