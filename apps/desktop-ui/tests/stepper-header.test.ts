import { afterEach, describe, expect, it, vi } from "vitest";

import StepperHeader from "@/components/ui/StepperHeader.vue";
import { click, findAll, mount, textOf, type Mounted, type TestNode } from "./helpers/test-renderer";

const steps = [
  { title: "Pick", hint: "first" },
  { title: "Check", hint: "second" },
  { title: "Run", hint: "third" },
];

let mounted: Mounted | undefined;

function show(props: { current: number; reachable?: number }): ReturnType<typeof vi.fn> {
  const onSelect = vi.fn();
  mounted = mount(StepperHeader, () => undefined, { steps, ...props, onSelect });
  return onSelect;
}

function stepButtons(): TestNode[] {
  if (!mounted) throw new Error("nothing mounted");
  return findAll(mounted.root, "button");
}

afterEach(() => {
  mounted?.unmount();
  mounted = undefined;
});

describe("StepperHeader", () => {
  it("makes each step before the current one a button that selects it", () => {
    const onSelect = show({ current: 2 });

    const buttons = stepButtons();
    expect(buttons.map((node) => textOf(node))).toEqual(["Pickfirst", "Checksecond"]);
    expect(buttons.every((node) => node.props.type === "button")).toBe(true);

    click(buttons[1] as TestNode);
    expect(onSelect).toHaveBeenCalledWith(1);
  });

  it("makes a reachable later step a button and leaves the current step plain", () => {
    const onSelect = show({ current: 0, reachable: 2 });

    const buttons = stepButtons();
    expect(buttons.map((node) => textOf(node))).toEqual(["2Checksecond", "3Runthird"]);

    click(buttons[1] as TestNode);
    expect(onSelect).toHaveBeenCalledWith(2);
  });

  it("has no buttons on the first step without a reachable one", () => {
    show({ current: 0 });

    expect(stepButtons()).toHaveLength(0);
  });
});
