import type { DependencyGraph, DependencySection } from "@/api/types";

export interface PlacedNode {
  name: string;
  layer: number;
  x: number;
  y: number;
  width: number;
  height: number;
}

export type Point = [number, number];

export interface PlacedEdge {
  consumer: string;
  library: string;
  section: DependencySection;
  back: boolean;
  points: Point[];
  path: string;
}

export interface GraphLayout {
  width: number;
  height: number;
  nodes: PlacedNode[];
  edges: PlacedEdge[];
  unlinked: string[];
}

export const ARROW_SIZE = 8;

const MARGIN = 8;
const COLUMN_GAP = 40;
const NODE_HEIGHT = 30;
const MIN_NODE_WIDTH = 96;
const MAX_NODE_WIDTH = 168;
const BOX_CLEARANCE = 16;
const ROUTE_GAP = 12;
const ROW_GAP = BOX_CLEARANCE - ARROW_SIZE;

interface GridNode {
  name: string;
  layer: number;
  row: number;
}

interface Route {
  index: number;
  library: GridNode;
  consumer: GridNode;
  kind: "next" | "long" | "back";
  boundary: number;
  track: number;
  approach: number;
  portOffset: number;
}

function bandSize(lanes: number): number {
  return 2 * BOX_CLEARANCE + (lanes - 1) * ROUTE_GAP;
}

function lanePosition(start: number, size: number, lanes: number, index: number): number {
  return Math.round(start + (size - (lanes - 1) * ROUTE_GAP) / 2) + index * ROUTE_GAP;
}

function centeredOffsets(count: number): number[] {
  return Array.from({ length: count }, (_, index) => (index - (count - 1) / 2) * ROUTE_GAP);
}

function offsetsAroundMiddle(count: number): number[] {
  const above = Math.ceil(count / 2);
  return Array.from({ length: count }, (_, index) => (index < above ? index - above : index - above + 1) * ROUTE_GAP);
}

function widest(offsets: number[]): number {
  return Math.max(0, ...offsets.map(Math.abs));
}

function pathOf(points: Point[]): string {
  return points.map(([x, y], index) => `${index === 0 ? "M" : "L"} ${x} ${y}`).join(" ");
}

function withoutRepeats(points: Point[]): Point[] {
  return points.filter((point, index) => {
    const previous = points[index - 1];
    return !previous || previous[0] !== point[0] || previous[1] !== point[1];
  });
}

export function layoutGraph(graph: DependencyGraph, width: number): GraphLayout {
  const linked = new Set(graph.edges.flatMap((edge) => [edge.consumer, edge.library]));
  const drawn = graph.nodes.filter((node) => linked.has(node.name));
  const unlinked = graph.nodes.filter((node) => !linked.has(node.name)).map((node) => node.name);

  const rowsPerLayer = new Map<number, number>();
  const grid: GridNode[] = drawn.map((node) => {
    const row = rowsPerLayer.get(node.layer) ?? 0;
    rowsPerLayer.set(node.layer, row + 1);
    return { name: node.name, layer: node.layer, row };
  });
  const byName = new Map(grid.map((node) => [node.name, node]));
  const columns = Math.max(1, ...grid.map((node) => node.layer + 1));
  const rows = Math.max(0, ...rowsPerLayer.values());

  const routes: Route[] = graph.edges.flatMap((edge, index) => {
    const library = byName.get(edge.library);
    const consumer = byName.get(edge.consumer);
    if (!library || !consumer) return [];
    const span = consumer.layer - library.layer;
    const kind = span === 1 ? "next" : span > 1 ? "long" : "back";
    const boundary = kind === "back" ? rows : consumer.row > library.row ? library.row + 1 : library.row;
    return [{ index, library, consumer, kind, boundary, track: 0, approach: 0, portOffset: 0 }];
  });

  const tracks = Array.from({ length: rows + 1 }, () => 0);
  for (const route of routes) {
    if (route.kind !== "next") route.track = tracks[route.boundary]!++;
  }

  const trunkSources = Array.from({ length: columns + 1 }, () => [] as string[]);
  const approaches = Array.from({ length: columns + 1 }, () => 0);
  for (const node of grid) {
    if (routes.some((route) => route.library === node)) trunkSources[node.layer + 1]!.push(node.name);
  }
  for (const route of routes) {
    if (route.kind !== "next") route.approach = approaches[route.consumer.layer]!++;
  }
  const lanesIn = (gap: number) => trunkSources[gap]!.length + approaches[gap]!;
  const startsBeside = (node: GridNode) =>
    grid.some(
      (other) =>
        other.layer === node.layer - 1 && other.row === node.row && trunkSources[node.layer]!.includes(other.name),
    );

  const arrival = (route: Route) => (route.kind === "next" ? 2 * route.library.row + 1 : 2 * route.boundary);
  const boxHeights = new Map<GridNode, number>();
  for (const node of grid) {
    const incoming = routes
      .filter((route) => route.consumer === node)
      .sort((a, b) => arrival(a) - arrival(b) || a.index - b.index);
    const beside = incoming.findIndex((route) => route.kind === "next" && route.library.row === node.row);
    const throughMiddle = incoming.map((_, port) => (port - beside) * ROUTE_GAP);
    const aroundMiddle = startsBeside(node) ? offsetsAroundMiddle(incoming.length) : centeredOffsets(incoming.length);
    const offsets = beside >= 0 && widest(throughMiddle) <= widest(aroundMiddle) ? throughMiddle : aroundMiddle;
    incoming.forEach((route, port) => (route.portOffset = offsets[port]!));
    boxHeights.set(node, Math.max(NODE_HEIGHT, 2 * (widest(offsets) + ARROW_SIZE)));
  }
  const rowHeights = Array.from({ length: rows }, (_, row) =>
    Math.max(NODE_HEIGHT, ...grid.filter((node) => node.row === row).map((node) => boxHeights.get(node)!)),
  );

  const gapWidths = Array.from({ length: columns + 1 }, (_, gap) => {
    const lanes = lanesIn(gap);
    const outer = gap === 0 || gap === columns;
    if (lanes === 0) return outer ? MARGIN : COLUMN_GAP;
    return Math.max(outer ? 0 : COLUMN_GAP, bandSize(lanes));
  });
  const gapsWidth = gapWidths.reduce((sum, gap) => sum + gap, 0);
  const nodeWidth = Math.floor(Math.min(MAX_NODE_WIDTH, Math.max(MIN_NODE_WIDTH, (width - gapsWidth) / columns)));
  const spare = width - gapsWidth - columns * nodeWidth;
  if (spare > 0 && columns > 1) {
    for (let gap = 1; gap < columns; gap++) {
      gapWidths[gap]! += Math.floor(spare / (columns - 1));
    }
  }
  const gapStarts: number[] = [];
  let x = 0;
  for (let gap = 0; gap <= columns; gap++) {
    gapStarts.push(x);
    x += gapWidths[gap]! + (gap < columns ? nodeWidth : 0);
  }
  const usedWidth = Math.max(width, x);
  const columnX = (layer: number) => gapStarts[layer]! + gapWidths[layer]!;

  const boundaryHeights = tracks.map((count, boundary) => {
    const outer = boundary === 0 || boundary === rows;
    if (count === 0) return outer ? MARGIN : ROW_GAP;
    return Math.max(outer ? 0 : ROW_GAP, bandSize(count));
  });
  const boundaryStarts: number[] = [];
  let y = 0;
  for (let boundary = 0; boundary <= rows; boundary++) {
    boundaryStarts.push(y);
    y += boundaryHeights[boundary]! + (boundary < rows ? rowHeights[boundary]! : 0);
  }
  const height = rows > 0 ? y : 0;
  const rowMiddle = (row: number) => boundaryStarts[row]! + boundaryHeights[row]! + rowHeights[row]! / 2;

  const nodes: PlacedNode[] = grid.map((node) => ({
    name: node.name,
    layer: node.layer,
    x: columnX(node.layer),
    y: rowMiddle(node.row) - boxHeights.get(node)! / 2,
    width: nodeWidth,
    height: boxHeights.get(node)!,
  }));

  const trunkX = (node: GridNode) => {
    const gap = node.layer + 1;
    return lanePosition(gapStarts[gap]!, gapWidths[gap]!, lanesIn(gap), trunkSources[gap]!.indexOf(node.name));
  };
  const approachX = (route: Route) => {
    const gap = route.consumer.layer;
    return lanePosition(gapStarts[gap]!, gapWidths[gap]!, lanesIn(gap), trunkSources[gap]!.length + route.approach);
  };
  const trackY = (route: Route) =>
    lanePosition(
      boundaryStarts[route.boundary]!,
      boundaryHeights[route.boundary]!,
      tracks[route.boundary]!,
      route.track,
    );

  const edges: PlacedEdge[] = routes.map((route) => {
    const edge = graph.edges[route.index]!;
    const startX = columnX(route.library.layer) + nodeWidth;
    const startY = rowMiddle(route.library.row);
    const endX = columnX(route.consumer.layer);
    const endY = rowMiddle(route.consumer.row) + route.portOffset;
    const trunk = trunkX(route.library);
    const middle: Point[] =
      route.kind === "next"
        ? [[trunk, endY]]
        : [
            [trunk, trackY(route)],
            [approachX(route), trackY(route)],
            [approachX(route), endY],
          ];
    const points = withoutRepeats([[startX, startY], [trunk, startY], ...middle, [endX, endY]]);
    return {
      consumer: edge.consumer,
      library: edge.library,
      section: edge.section,
      back: route.kind === "back",
      points,
      path: pathOf(points),
    };
  });

  return { width: usedWidth, height, nodes, edges, unlinked };
}
