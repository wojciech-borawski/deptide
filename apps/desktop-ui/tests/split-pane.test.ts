import { afterEach, describe, expect, it, vi } from "vitest";
import { defineComponent, h, nextTick, ref } from "vue";

type ResizeCallback = (entries: { contentRect: { width: number } }[]) => void;

const resize = vi.hoisted(() => ({ callbacks: [] as ResizeCallback[] }));

vi.mock("@vueuse/core", async (original) => ({
  ...(await original<object>()),
  useResizeObserver: (_target: unknown, callback: ResizeCallback) => {
    resize.callbacks.push(callback);
  },
}));

import SplitPane from "@/components/ui/SplitPane.vue";
import { dispatch, mount, type Mounted, type TestNode } from "./helpers/test-renderer";

const containerLeft = 100;
const containerWidth = 1000;

let mounted: Mounted | undefined;
let updates: number[] = [];
let value = ref(32);

function walk(node: TestNode): TestNode[] {
  return [node, ...node.children.flatMap(walk)];
}

function show(options: { end?: boolean; model?: number; stackBelow?: number } = {}): void {
  updates = [];
  resize.callbacks = [];
  value = ref(options.model ?? 32);
  const Host = defineComponent({
    render: () =>
      h(
        SplitPane,
        {
          modelValue: value.value,
          label: "Resize panes",
          stackBelow: options.stackBelow ?? 0,
          "onUpdate:modelValue": (next: number) => {
            updates.push(next);
            value.value = next;
          },
        },
        {
          start: () => h("div", "list"),
          ...(options.end === false ? {} : { end: () => h("div", "preview") }),
        },
      ),
  });
  mounted = mount(Host, () => undefined);
}

function root(): TestNode {
  if (!mounted) throw new Error("nothing mounted");
  const pane = walk(mounted.root).find((node) => String(node.props.class ?? "").includes("split"));
  if (!pane) throw new Error("no split pane");
  return pane;
}

function handle(): TestNode {
  const found = walk(root()).find((node) => node.props.role === "separator");
  if (!found) throw new Error("no separator");
  return found;
}

function withGeometry(): TestNode {
  const container = root();
  Object.assign(container, {
    getBoundingClientRect: () => ({ left: containerLeft, width: containerWidth }),
  });
  const grip = handle();
  Object.assign(grip, { setPointerCapture: vi.fn(), releasePointerCapture: vi.fn() });
  return grip;
}

async function resizeTo(width: number): Promise<void> {
  for (const callback of resize.callbacks) callback([{ contentRect: { width } }]);
  await nextTick();
}

async function press(grip: TestNode, key: string, shiftKey = false): Promise<void> {
  dispatch(grip, "keydown", { key, shiftKey });
  await nextTick();
}

afterEach(() => {
  mounted?.unmount();
  mounted = undefined;
});

describe("SplitPane separator", () => {
  it("is a focusable vertical separator that reports the split in percent", async () => {
    show();
    const grip = handle();

    expect(grip.props["aria-orientation"]).toBe("vertical");
    expect(grip.props["aria-label"]).toBe("Resize panes");
    expect(grip.props.tabindex).toBe(0);
    expect(grip.props["aria-valuenow"]).toBe(32);
    expect(grip.props["aria-valuemin"]).toBe(5);
    expect(grip.props["aria-valuemax"]).toBe(95);

    withGeometry();
    await press(grip, "ArrowRight");
    await nextTick();

    expect(handle().props["aria-valuenow"]).toBe(34);
    expect(handle().props["aria-valuemin"]).toBe(22);
    expect(handle().props["aria-valuemax"]).toBe(62);
  });

  it("is absent when only the start pane is given", () => {
    show({ end: false });

    expect(walk(root()).some((node) => node.props.role === "separator")).toBe(false);
  });

  it("stacks the panes without a separator while narrower than stackBelow", async () => {
    show({ stackBelow: 1100 });
    expect(resize.callbacks).toHaveLength(1);

    await resizeTo(1000);
    expect(String(root().props.class)).toContain("stacked");
    expect(walk(root()).some((node) => node.props.role === "separator")).toBe(false);

    await resizeTo(1200);
    expect(String(root().props.class)).not.toContain("stacked");
    expect(handle().props["aria-orientation"]).toBe("vertical");
  });
});

describe("SplitPane keyboard", () => {
  it("moves by 2 percent with the arrows and by 10 with Shift", async () => {
    show();
    const grip = withGeometry();

    await press(grip, "ArrowRight");
    await press(grip, "ArrowRight");
    await press(grip, "ArrowLeft");
    await press(grip, "ArrowRight", true);
    await press(grip, "ArrowLeft", true);
    await nextTick();

    expect(updates).toEqual([34, 36, 34, 44, 34]);
  });

  it("stops at the pixel minimums of both panes", async () => {
    show({ model: 60 });
    const grip = withGeometry();

    await press(grip, "ArrowRight", true);
    expect(updates).toEqual([62.6]);
    await press(grip, "ArrowRight");
    expect(updates).toEqual([62.6]);

    value.value = 25;
    await nextTick();
    await press(grip, "ArrowLeft", true);
    expect(updates).toEqual([62.6, 22]);
    await press(grip, "ArrowLeft");
    expect(updates).toEqual([62.6, 22]);
  });

  it("jumps to the smallest and largest split with Home and End", async () => {
    show();
    const grip = withGeometry();

    await press(grip, "End");
    await press(grip, "Home");

    expect(updates).toEqual([62.6, 22]);
  });

  it("ignores other keys and leaves them to the browser", () => {
    show();
    const grip = withGeometry();
    const preventDefault = vi.fn();

    dispatch(grip, "keydown", { key: "a", preventDefault });
    dispatch(grip, "keydown", { key: "ArrowRight", preventDefault });

    expect(updates).toEqual([34]);
    expect(preventDefault).toHaveBeenCalledTimes(1);
  });

  it("leaves Alt with an arrow to the wizard step keys", async () => {
    show();
    const grip = withGeometry();
    const preventDefault = vi.fn();

    dispatch(grip, "keydown", { key: "ArrowRight", altKey: true, preventDefault });
    dispatch(grip, "keydown", { key: "ArrowLeft", altKey: true, preventDefault });
    await nextTick();

    expect(updates).toEqual([]);
    expect(preventDefault).not.toHaveBeenCalled();
  });
});

describe("SplitPane reset", () => {
  it("returns to the default on double click", () => {
    show({ model: 50 });
    const grip = withGeometry();

    dispatch(grip, "dblclick");

    expect(updates).toEqual([32]);
  });
});

describe("SplitPane drag", () => {
  it("follows the pointer with the handle centred under it and clamps at both ends", async () => {
    show();
    const grip = withGeometry();

    dispatch(grip, "pointerdown", { button: 0, pointerId: 7, clientX: 430 });
    await nextTick();
    expect(String(root().props.class)).toContain("dragging");
    expect((grip as unknown as { setPointerCapture: ReturnType<typeof vi.fn> }).setPointerCapture).toHaveBeenCalledWith(
      7,
    );

    dispatch(grip, "pointermove", { pointerId: 7, clientX: 500 });
    dispatch(grip, "pointermove", { pointerId: 7, clientX: 5000 });
    dispatch(grip, "pointermove", { pointerId: 7, clientX: -5000 });

    expect(updates).toEqual([39.3, 62.6, 22]);
  });

  it("stops following after the pointer is released", async () => {
    show();
    const grip = withGeometry();

    dispatch(grip, "pointerdown", { button: 0, pointerId: 1, clientX: 430 });
    await nextTick();
    expect(String(root().props.class)).toContain("dragging");
    dispatch(grip, "pointerup", { pointerId: 1 });
    await nextTick();
    dispatch(grip, "pointermove", { pointerId: 1, clientX: 600 });

    expect(String(root().props.class)).not.toContain("dragging");
    expect(updates).toEqual([]);
  });

  it("ignores moves that did not start on the handle and non-primary buttons", () => {
    show();
    const grip = withGeometry();

    dispatch(grip, "pointermove", { pointerId: 1, clientX: 600 });
    dispatch(grip, "pointerdown", { button: 2, pointerId: 1, clientX: 430 });
    dispatch(grip, "pointermove", { pointerId: 1, clientX: 600 });

    expect(updates).toEqual([]);
  });

  it("stops selecting text by cancelling the press", () => {
    show();
    const grip = withGeometry();
    const preventDefault = vi.fn();

    dispatch(grip, "pointerdown", { button: 0, pointerId: 1, clientX: 430, preventDefault });

    expect(preventDefault).toHaveBeenCalled();
  });
});
