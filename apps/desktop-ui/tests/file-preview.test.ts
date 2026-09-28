import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { nextTick, type App } from "vue";
import { createPinia, setActivePinia, type Pinia } from "pinia";

import type { FileSide, FileStatus, ReceiveFileContents } from "@/api/types";

const backend = vi.hoisted(() => ({ readReceiveFile: vi.fn() }));

vi.mock("@/api/commands", () => ({ ...backend, reportError: () => undefined }));
vi.mock("vue-router", () => ({ useRouter: () => ({ push: vi.fn() }) }));
vi.mock("@/router", () => ({ routeNames: { settings: "settings" } }));

import FilePreview from "@/components/transfer/FilePreview.vue";
import ReceiveProjectTab from "@/components/transfer/ReceiveProjectTab.vue";
import { i18n } from "@/i18n";
import { rowLimit } from "@/lib/diff-view";
import { useTransferStore } from "@/stores/transfer";
import { useUiStore } from "@/stores/ui";
import { file, makeProject } from "./helpers/receive-fixtures";
import {
  buttonLabelled,
  click,
  dispatch,
  findAll,
  focused,
  mount,
  testDocument,
  textOf,
  type Mounted,
  type TestNode,
} from "./helpers/test-renderer";

const source = "C:\\clip\\web";
const largeNotice = "This diff is large";

let pinia: Pinia;
let mounted: Mounted | undefined;

function install(app: App<TestNode>): void {
  app.use(pinia);
  app.use(i18n);
  app.directive("ripple", {});
}

function side(text: string): FileSide {
  return { kind: "text", text, bom: false, size: text.length };
}

function numbered(count: number, prefix = "line"): string[] {
  return Array.from({ length: count }, (_, index) => `${prefix} ${index + 1}`);
}

function text(lines: readonly string[]): string {
  return lines.map((line) => `${line}\n`).join("");
}

function root(): TestNode {
  if (!mounted) throw new Error("nothing mounted");
  return mounted.root;
}

function showPreview(relative: string, status: FileStatus, contents: ReceiveFileContents, onClose = vi.fn()): void {
  backend.readReceiveFile.mockResolvedValue(contents);
  mounted = mount(FilePreview, install, { source, file: file(relative, status), onClose });
}

function button(label: string): TestNode {
  const found = buttonLabelled(root(), label);
  if (!found) throw new Error(`no button ${label}`);
  return found;
}

function codeHtml(): string {
  return findAll(root(), "span")
    .map((node) => String(node.props.innerHTML ?? ""))
    .join("\n");
}

beforeEach(() => {
  backend.readReceiveFile.mockReset();
  pinia = createPinia();
  setActivePinia(pinia);
  i18n.global.locale.value = "en";
  vi.stubGlobal("document", testDocument);
});

afterEach(() => {
  mounted?.unmount();
  mounted = undefined;
  vi.unstubAllGlobals();
});

describe("FilePreview limits", () => {
  it("shows a notice instead of a diff over the row limit, then renders it and keeps focus in the preview", async () => {
    const onClose = vi.fn();
    showPreview("big.txt", "added", { local: null, received: side(text(numbered(rowLimit + 1))) }, onClose);
    await vi.waitFor(() => expect(textOf(root())).toContain(largeNotice));
    expect(findAll(root(), "tr")).toHaveLength(0);

    const showAnyway = button("Show diff anyway");
    showAnyway.focus();
    click(showAnyway);
    await nextTick();

    expect(textOf(root())).not.toContain(largeNotice);
    expect(findAll(root(), "tr")).toHaveLength(rowLimit + 1);
    const section = findAll(root(), "section")[0];
    expect(focused()).toBe(section);

    dispatch(section as TestNode, "keydown", { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("decides on the rows of the view on screen", async () => {
    useUiStore().diffMode = "split";
    showPreview("rewrite.txt", "replaced", {
      local: side(text(numbered(3000))),
      received: side(text(numbered(3000, "new"))),
    });
    await vi.waitFor(() => expect(findAll(root(), "tr").length).toBeGreaterThan(3000));
    expect(textOf(root())).not.toContain(largeNotice);

    useUiStore().diffMode = "unified";
    await nextTick();

    expect(textOf(root())).toContain(largeNotice);
    expect(findAll(root(), "tr")).toHaveLength(0);
  });

  it("shows the notice again when opening a gap would pass the row limit", async () => {
    const before = numbered(rowLimit + 10);
    showPreview("long.txt", "replaced", {
      local: side(text(before)),
      received: side(text([...before.slice(0, -1), "changed"])),
    });
    await vi.waitFor(() => expect(textOf(root())).toContain("unchanged lines"));

    click(button("unchanged lines"));
    await nextTick();

    expect(textOf(root())).toContain(largeNotice);

    click(button("Show diff anyway"));
    await nextTick();

    expect(findAll(root(), "tr")).toHaveLength(rowLimit + 11);
  });

  it("highlights a small file without a note", async () => {
    const code = 'const name = "a";\nconst other = 2;\n';
    showPreview("small.ts", "replaced", { local: side(code), received: side(code.replace("2", "3")) });
    await vi.waitFor(() => expect(findAll(root(), "tr").length).toBeGreaterThan(0));

    expect(codeHtml()).toContain("hljs-keyword");
    expect(textOf(root())).not.toContain("Highlighting is off");
  });

  it("turns highlighting off with a note when one side is over 256 KiB", async () => {
    const lines = Array.from({ length: 12000 }, (_, index) => `const value${index} = "${index}";`);
    const before = text(lines);
    const after = text([...lines.slice(0, -1), "const changed = 1;"]);
    expect(before.length).toBeGreaterThan(256 * 1024);
    showPreview("big.ts", "replaced", { local: side(before), received: side(after) });
    await vi.waitFor(() => expect(findAll(root(), "tr").length).toBeGreaterThan(0));

    expect(textOf(root())).toContain("Highlighting is off for large files");
    expect(codeHtml()).not.toContain("hljs");
  });

  it("closes on Escape pressed anywhere inside", async () => {
    const onClose = vi.fn();
    showPreview("a.txt", "added", { local: null, received: side("hello\n") }, onClose);
    await vi.waitFor(() => expect(findAll(root(), "tr")).toHaveLength(1));
    const close = findAll(root(), "button").find((node) => node.props["aria-label"] === "Close preview");

    dispatch(close as TestNode, "keydown", { key: "Escape" });

    expect(onClose).toHaveBeenCalledTimes(1);
  });
});

describe("ReceiveProjectTab preview", () => {
  it("puts focus back on the file tree when the preview closes from inside", async () => {
    const project = makeProject(source, "web", [file("a.txt", "added"), file("b.txt", "added")]);
    backend.readReceiveFile.mockResolvedValue({ local: null, received: side("hello\n") });
    useUiStore().receiveLayout = "tree";
    const transfer = useTransferStore();
    transfer.setPreview(source, "a.txt");
    mounted = mount(ReceiveProjectTab, install, { project });
    await vi.waitFor(() => expect(findAll(root(), "section")).toHaveLength(1));
    const close = findAll(root(), "button").find((node) => node.props["aria-label"] === "Close preview");

    close?.focus();
    click(close as TestNode);
    await nextTick();

    expect(transfer.previewFor(source)).toBeNull();
    expect(focused()?.props.role).toBe("tree");
  });
});
