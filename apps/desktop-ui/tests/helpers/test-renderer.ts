import { createRenderer, type App, type Component } from "vue";

export interface TestNode {
  tag: string;
  text: string;
  props: Record<string, unknown>;
  children: TestNode[];
  parent: TestNode | null;
  addEventListener: () => void;
  focus: () => void;
  contains: (other: TestNode | null) => boolean;
  querySelector: (selector: string) => TestNode | null;
  scrollIntoView: () => void;
}

export interface Mounted {
  root: TestNode;
  unmount: () => void;
}

const focusableTags = new Set(["a", "button", "input", "select", "textarea"]);

let active: TestNode | null = null;

/** Stands in for `document` in tests: `vi.stubGlobal("document", testDocument)`. */
export const testDocument = {
  documentElement: { lang: "", setAttribute: () => undefined, removeAttribute: () => undefined },
  get activeElement(): TestNode | null {
    return active;
  },
};

/** The node that took focus last, or `null` once it left the tree, like focus falling back to `<body>`. */
export function focused(): TestNode | null {
  return active;
}

function isFocusable(node: TestNode): boolean {
  if (node.props.disabled) return false;
  return focusableTags.has(node.tag) || node.props.tabindex !== undefined;
}

function matches(node: TestNode, selector: string): boolean {
  const id = /^#([\w-]+)$/.exec(selector);
  if (id) return node.props.id === id[1];
  const attribute = /^\[([\w-]+)="([^"]*)"\]$/.exec(selector);
  return !!attribute && node.props[attribute[1] ?? ""] === attribute[2];
}

function descendants(node: TestNode): TestNode[] {
  return node.children.flatMap((child) => [child, ...descendants(child)]);
}

function createNode(tag: string, text = ""): TestNode {
  const node: TestNode = {
    tag,
    text,
    props: {},
    children: [],
    parent: null,
    addEventListener: () => undefined,
    focus: () => {
      if (isFocusable(node)) active = node;
    },
    contains: (other) => {
      for (let current = other; current; current = current.parent) if (current === node) return true;
      return false;
    },
    querySelector: (selector) => {
      const options = selector.split(",").map((part) => part.trim());
      return descendants(node).find((child) => options.some((option) => matches(child, option))) ?? null;
    },
    scrollIntoView: () => undefined,
  };
  return node;
}

function detach(child: TestNode): void {
  const siblings = child.parent?.children;
  if (siblings) siblings.splice(siblings.indexOf(child), 1);
  child.parent = null;
  if (child.contains(active)) active = null;
}

function insert(child: TestNode, parent: TestNode, anchor?: TestNode | null): void {
  detach(child);
  const index = anchor ? parent.children.indexOf(anchor) : -1;
  if (index === -1) parent.children.push(child);
  else parent.children.splice(index, 0, child);
  child.parent = parent;
}

const renderer = createRenderer<TestNode, TestNode>({
  createElement: (tag) => createNode(tag),
  createText: (text) => createNode("#text", text),
  createComment: (text) => createNode("#comment", text),
  setText: (node, text) => {
    node.text = text;
  },
  setElementText: (node, text) => {
    for (const child of [...node.children]) detach(child);
    if (text) insert(createNode("#text", text), node);
  },
  insert,
  remove: detach,
  parentNode: (node) => node.parent,
  nextSibling: (node) => {
    const siblings = node.parent?.children ?? [];
    return siblings[siblings.indexOf(node) + 1] ?? null;
  },
  patchProp: (node, key, _previous, next) => {
    node.props[key] = next;
  },
});

export function mount(
  component: Component,
  install: (app: App<TestNode>) => void,
  props: Record<string, unknown> = {},
): Mounted {
  const root = createNode("#root");
  const app = renderer.createApp(component, props);
  install(app);
  app.mount(root);
  return { root, unmount: () => app.unmount() };
}

export function textOf(node: TestNode): string {
  if (node.tag === "#text") return node.text;
  if (node.tag === "#comment") return "";
  return node.children.map(textOf).join("");
}

export function findAll(node: TestNode, tag: string): TestNode[] {
  const own = node.tag === tag ? [node] : [];
  return [...own, ...node.children.flatMap((child) => findAll(child, tag))];
}

export function buttonLabelled(root: TestNode, label: string): TestNode | undefined {
  return findAll(root, "button").find((button) => textOf(button).includes(label));
}

/**
 * Calls the `on<Name>` handlers of `node` and then of each ancestor, like a bubbling DOM event, until one calls
 * `stopPropagation`, e.g. `dispatch(tab, "keydown", { key: "ArrowRight" })`.
 */
export function dispatch(node: TestNode, name: string, event: Record<string, unknown> = {}): void {
  const key = `on${name.charAt(0).toUpperCase()}${name.slice(1)}`;
  let stopped = false;
  const full: Record<string, unknown> = {
    preventDefault: () => undefined,
    stopPropagation: () => {
      stopped = true;
    },
    target: node,
    ...event,
  };
  for (let current: TestNode | null = node; current && !stopped; current = current.parent) {
    full.currentTarget = current;
    const handlers = [current.props[key]].flat().filter((handler) => typeof handler === "function");
    for (const handler of handlers) (handler as (value: object) => unknown)(full);
  }
}

/** Clicks `node` like a browser would, refusing a disabled element. */
export function click(node: TestNode): void {
  if (node.props.disabled) throw new Error(`<${node.tag}> is disabled`);
  dispatch(node, "click");
}
