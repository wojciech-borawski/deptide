import { afterEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia, type Pinia } from "pinia";
import { createSSRApp, h, nextTick, type Component } from "vue";
import { renderToString } from "vue/server-renderer";

vi.mock("vue-router", () => ({ useRouter: () => ({ push: vi.fn() }) }));
vi.mock("@/router", () => ({ routeNames: { settings: "settings" } }));

import IgnorePatternChips from "@/components/transfer/IgnorePatternChips.vue";
import ReceiveFileList from "@/components/transfer/ReceiveFileList.vue";
import ReceiveFileTree from "@/components/transfer/ReceiveFileTree.vue";
import ReceiveTabs from "@/components/transfer/ReceiveTabs.vue";
import { i18n } from "@/i18n";
import { useTransferStore } from "@/stores/transfer";
import { file, makeProject } from "./helpers/receive-fixtures";
import { dispatch, findAll, mount, textOf, type Mounted, type TestNode } from "./helpers/test-renderer";

const projects = [
  makeProject("C:\\clip\\web", "web", [file("a.ts", "added")]),
  makeProject("C:\\clip\\api", "api", [file("b.ts", "replaced")]),
];

async function render(component: Component, props: Record<string, unknown>, pinia?: Pinia): Promise<string> {
  const app = createSSRApp({ render: () => h(component, props, { default: () => "panel body" }) });
  app.use(i18n);
  if (pinia) app.use(pinia);
  return renderToString(app);
}

function tags(html: string, name: string): string[] {
  return html.match(new RegExp(`<${name}\\b[^>]*>`, "g")) ?? [];
}

function roles(html: string): string[] {
  return [...html.matchAll(/\brole="([^"]+)"/g)].map((match) => match[1] ?? "");
}

describe("ReceiveTabs", () => {
  it("puts only tabs in the tablist and hides the close buttons from the tab order and assistive tech", async () => {
    const html = await render(ReceiveTabs, { projects, active: projects[1]?.source, done: [] });

    expect(roles(html)).toEqual(["tablist", "presentation", "tab", "presentation", "tab", "tabpanel"]);
    const closeButtons = tags(html, "button").filter((tag) => !tag.includes('role="tab"'));
    expect(closeButtons).toHaveLength(2);
    for (const tag of closeButtons) {
      expect(tag).toContain('tabindex="-1"');
      expect(tag).toContain('aria-hidden="true"');
    }
  });

  it("makes only the active tab reachable with Tab and labels the panel with it", async () => {
    const html = await render(ReceiveTabs, { projects, active: projects[1]?.source, done: [] });
    const tabs = tags(html, "button").filter((tag) => tag.includes('role="tab"'));

    expect(tabs.map((tag) => /tabindex="(-?\d)"/.exec(tag)?.[1])).toEqual(["-1", "0"]);
    expect(tabs[1]).toContain('id="receive-tab-1"');
    expect(tabs[1]).toContain('aria-selected="true"');
    expect(tags(html, "div").find((tag) => tag.includes('role="tabpanel"'))).toContain(
      'aria-labelledby="receive-tab-1"',
    );
    expect(html).toContain("panel body");
  });

  it("leaves the panel unlabelled and the first tab reachable when no tab is active", async () => {
    const html = await render(ReceiveTabs, { projects, active: null, done: [] });
    const tabs = tags(html, "button").filter((tag) => tag.includes('role="tab"'));

    expect(html).not.toContain("aria-labelledby");
    expect(tabs.map((tag) => /tabindex="(-?\d)"/.exec(tag)?.[1])).toEqual(["0", "-1"]);
  });
});

describe("IgnorePatternChips", () => {
  it("disables the pattern chips while busy and keeps the Settings link usable", async () => {
    const html = await render(IgnorePatternChips, { patterns: ["*.log", "dist/"], disabled: [], busy: true });
    const buttons = tags(html, "button");

    expect(buttons).toHaveLength(3);
    expect(buttons.slice(0, 2).every((tag) => /\bdisabled\b/.test(tag))).toBe(true);
    expect(buttons[2]).not.toMatch(/\bdisabled\b/);
  });

  it("leaves the pattern chips enabled when not busy", async () => {
    const html = await render(IgnorePatternChips, { patterns: ["*.log"], disabled: [] });

    expect(tags(html, "button").some((tag) => /\bdisabled\b/.test(tag))).toBe(false);
  });
});

describe("file list keyboard semantics", () => {
  const source = "C:\\clip\\web";
  const files = [file("b.ts", "replaced"), file("a.ts", "added"), file("src/c.ts", "removed")];

  function withPreview(relative: string | null): Pinia {
    const pinia = createPinia();
    setActivePinia(pinia);
    const store = useTransferStore();
    store.setPreview(source, relative);
    store.toggleFolder(source, "src");
    return pinia;
  }

  function attribute(tag: string | undefined, name: string): string | undefined {
    return tag ? new RegExp(`\\b${name}="([^"]*)"`).exec(tag)?.[1] : undefined;
  }

  it("renders the list as a labelled listbox whose active option is the previewed row", async () => {
    const html = await render(ReceiveFileList, { source, files, sort: "path" }, withPreview("b.ts"));
    const listbox = tags(html, "div").find((tag) => tag.includes('role="listbox"'));
    const options = tags(html, "div").filter((tag) => tag.includes('role="option"'));

    expect(attribute(listbox, "aria-label")).toBeTruthy();
    expect(attribute(listbox, "tabindex")).toBe("0");
    expect(options.map((tag) => attribute(tag, "aria-selected"))).toEqual(["false", "true", "false"]);
    expect(attribute(listbox, "aria-activedescendant")).toBe(attribute(options[1], "id"));
    expect(new Set(options.map((tag) => attribute(tag, "id"))).size).toBe(3);
  });

  it("points at no row while nothing is previewed", async () => {
    const html = await render(ReceiveFileList, { source, files, sort: "path" }, withPreview(null));
    const listbox = tags(html, "div").find((tag) => tag.includes('role="listbox"'));

    expect(listbox).not.toContain("aria-activedescendant");
    expect(tags(html, "div").filter((tag) => tag.includes('aria-selected="true"'))).toEqual([]);
  });

  it("groups the options under their status heading when sorted by change", async () => {
    const html = await render(ReceiveFileList, { source, files, sort: "change" }, withPreview(null));
    const groups = tags(html, "div").filter((tag) => tag.includes('role="group"'));

    expect(groups).toHaveLength(3);
    for (const group of groups) {
      expect(html).toContain(`id="${attribute(group, "aria-labelledby")}"`);
    }
  });

  it("renders the tree as a labelled tree with levels, expanded folders and the previewed file selected", async () => {
    const html = await render(ReceiveFileTree, { source, files }, withPreview("src/c.ts"));
    const tree = tags(html, "div").find((tag) => tag.includes('role="tree"'));
    const items = tags(html, "div").filter((tag) => tag.includes('role="treeitem"'));

    const folder = items.find((tag) => tag.includes("aria-expanded"));
    const previewed = items.find((tag) => tag.includes('aria-selected="true"'));

    expect(attribute(tree, "aria-label")).toBeTruthy();
    expect(items).toHaveLength(4);
    expect(items.map((tag) => attribute(tag, "aria-level")).sort()).toEqual(["1", "1", "1", "2"]);
    expect(attribute(folder, "aria-expanded")).toBe("true");
    expect(attribute(folder, "aria-level")).toBe("1");
    expect(attribute(previewed, "aria-level")).toBe("2");
    expect(attribute(previewed, "title")).toBe("src/c.ts");
    expect(attribute(tree, "aria-activedescendant")).toBe(attribute(previewed, "id"));
  });

  it("renders a collapsed folder as not expanded", async () => {
    const pinia = createPinia();
    setActivePinia(pinia);
    const html = await render(ReceiveFileTree, { source, files }, pinia);
    const folder = tags(html, "div").find((tag) => tag.includes('role="treeitem"') && tag.includes("aria-expanded"));

    expect(attribute(folder, "aria-expanded")).toBe("false");
  });

  it("names each option once by its path and status and describes the previewed one", async () => {
    const html = await render(ReceiveFileList, { source, files, sort: "path" }, withPreview("b.ts"));
    const options = tags(html, "div").filter((tag) => tag.includes('role="option"'));
    const hint = attribute(options[1], "aria-describedby");

    expect(options.map((tag) => attribute(tag, "aria-label"))).toEqual([
      "a.ts, new",
      "b.ts, replaced",
      "src/c.ts, removed",
    ]);
    expect(hint).toBeTruthy();
    expect(new RegExp(`id="${hint}"[^>]*>Shown in the preview<`).test(html)).toBe(true);
    expect(attribute(options[0], "aria-describedby")).toBeUndefined();
    expect(attribute(options[2], "aria-describedby")).toBeUndefined();
  });

  it("gives rows of two lists on screen at once different ids", async () => {
    const pinia = withPreview(null);
    const app = createSSRApp({
      render: () => [
        h(ReceiveFileList, { source, files, sort: "path" }),
        h(ReceiveFileList, { source: "C:\\clip\\api", files, sort: "path" }),
      ],
    });
    app.use(i18n);
    app.use(pinia);
    const html = await renderToString(app);
    const ids = tags(html, "div")
      .filter((tag) => tag.includes('role="option"'))
      .map((tag) => attribute(tag, "id"));

    expect(ids).toHaveLength(6);
    expect(new Set(ids).size).toBe(6);
  });
});

describe("file list keyboard", () => {
  const source = "C:\\clip\\web";
  const files = [file("b.ts", "replaced"), file("a.ts", "added"), file("src/c.ts", "removed")];
  let mounted: Mounted | undefined;

  afterEach(() => mounted?.unmount());

  function show(component: Component, props: Record<string, unknown>): ReturnType<typeof useTransferStore> {
    const pinia = createPinia();
    setActivePinia(pinia);
    mounted = mount(
      component,
      (app) => {
        app.use(pinia);
        app.use(i18n);
      },
      props,
    );
    return useTransferStore();
  }

  function container(): TestNode {
    const found = findAll(mounted?.root as TestNode, "div").find((node) =>
      ["listbox", "tree"].includes(String(node.props.role)),
    );
    if (!found) throw new Error("no list");
    return found;
  }

  function rowNamed(name: string): TestNode {
    const found = findAll(container(), "div").find(
      (node) => node.props.title === name || node.props["aria-label"] === name,
    );
    if (!found) throw new Error(`no row ${name}`);
    return found;
  }

  function cursorRow(): string | undefined {
    const id = container().props["aria-activedescendant"];
    const row = findAll(container(), "div").find((node) => id !== undefined && node.props.id === id);
    return row ? String(row.props.title ?? row.props["aria-label"]) : undefined;
  }

  function selectedRows(): string[] {
    return findAll(container(), "div")
      .filter((node) => node.props["aria-selected"] === true)
      .map((node) => String(node.props.title));
  }

  function press(key: string, on: TestNode = container()): void {
    dispatch(on, "keydown", { key });
  }

  it("moves the cursor without opening the preview while it is closed", async () => {
    const transfer = show(ReceiveFileList, { source, files, sort: "path" });

    press("ArrowDown");
    press("ArrowDown");
    await nextTick();

    expect(cursorRow()).toBe("b.ts");
    expect(selectedRows()).toEqual([]);
    expect(transfer.previewFor(source)).toBeNull();
  });

  it("opens the preview with Enter, takes it along with the arrows and closes it with Escape", async () => {
    const transfer = show(ReceiveFileList, { source, files, sort: "path" });

    press("ArrowDown");
    press("Enter");
    press("ArrowDown");
    await nextTick();

    expect(transfer.previewFor(source)).toBe("b.ts");
    expect(cursorRow()).toBe("b.ts");
    expect(selectedRows()).toEqual(["b.ts"]);

    press("Escape");
    press("ArrowUp");
    await nextTick();

    expect(transfer.previewFor(source)).toBeNull();
    expect(cursorRow()).toBe("a.ts");
    expect(selectedRows()).toEqual([]);
  });

  it("ignores keys pressed on a checkbox inside a row", async () => {
    const transfer = show(ReceiveFileList, { source, files, sort: "path" });
    transfer.setPreview(source, "a.ts");
    await nextTick();
    const checkbox = findAll(rowNamed("a.ts"), "input")[0] as TestNode;

    press("ArrowDown", checkbox);
    await nextTick();

    expect(transfer.previewFor(source)).toBe("a.ts");
    expect(cursorRow()).toBe("a.ts");
  });

  it("walks folders in the tree, folds them with Left and Right and keeps the preview on its file", async () => {
    const transfer = show(ReceiveFileTree, { source, files });
    transfer.toggleFolder(source, "src");
    transfer.setPreview(source, "src/c.ts");
    await nextTick();

    press("ArrowLeft");
    await nextTick();
    expect(cursorRow()).toBe("src");
    expect(selectedRows()).toEqual(["src/c.ts"]);

    press("ArrowLeft");
    await nextTick();
    expect(transfer.isExpanded(source, "src")).toBe(false);
    expect(rowNamed("src").props["aria-expanded"]).toBe(false);
    expect(textOf(container())).not.toContain("c.ts");

    press("ArrowRight");
    press("ArrowRight");
    await nextTick();
    expect(transfer.isExpanded(source, "src")).toBe(true);
    expect(cursorRow()).toBe("src/c.ts");

    press("ArrowDown");
    press("ArrowDown");
    await nextTick();
    expect(cursorRow()).toBe("b.ts");
    expect(transfer.previewFor(source)).toBe("b.ts");
  });
});
