import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { type App } from "vue";
import { createPinia, setActivePinia, type Pinia } from "pinia";

vi.mock("@/api/commands", () => ({ reportError: () => undefined, cancelDependencyRequest: vi.fn() }));

import DependencyGraphTab from "@/components/dependencies/DependencyGraphTab.vue";
import { mockDependencyGraph } from "@/dev/mock/dependencies";
import { i18n } from "@/i18n";
import { useDependenciesStore } from "@/stores/dependencies";
import { findAll, mount, testDocument, type Mounted, type TestNode } from "./helpers/test-renderer";

let pinia: Pinia;
let mounted: Mounted | undefined;

function install(app: App): void {
  app.use(pinia);
  app.use(i18n);
  app.directive("ripple", {});
}

function nodes(root: TestNode): TestNode[] {
  return findAll(root, "g").filter((group) => group.props.role === "button");
}

beforeEach(() => {
  pinia = createPinia();
  setActivePinia(pinia);
  i18n.global.locale.value = "en";
  vi.stubGlobal("document", testDocument);
  useDependenciesStore().graph = mockDependencyGraph();
  mounted = mount(DependencyGraphTab as never, install as never);
});

afterEach(() => {
  mounted?.unmount();
  mounted = undefined;
  vi.unstubAllGlobals();
});

describe("dependency graph tab", () => {
  it("ends every edge in an arrowhead defined in the same picture", () => {
    const picture = findAll(mounted!.root, "svg").find((svg) => svg.props.role === "group")!;
    const markers = findAll(picture, "marker").map((marker) => marker.props.id as string);
    const edges = picture.children.filter((child) => child.tag === "path");

    expect(edges.length).toBeGreaterThan(0);
    for (const edge of edges) {
      const reference = /^url\(#(.+)\)$/.exec(String(edge.props["marker-end"]))?.[1];
      expect(markers).toContain(reference);
    }
  });

  it("tells assistive technology which node is pinned", async () => {
    const [first, second] = nodes(mounted!.root);
    const pressed = () => nodes(mounted!.root).map((node) => node.props["aria-pressed"]);
    const before = pressed();

    (first!.props.onClick as () => void)();
    await Promise.resolve();
    const afterFirst = pressed();
    (second!.props.onClick as () => void)();
    await Promise.resolve();
    const afterSecond = pressed();
    (second!.props.onClick as () => void)();
    await Promise.resolve();

    expect(before.every((value) => value === false)).toBe(true);
    expect(afterFirst.slice(0, 2)).toEqual([true, false]);
    expect(afterSecond.slice(0, 2)).toEqual([false, true]);
    expect(pressed().every((value) => value === false)).toBe(true);
  });
});
