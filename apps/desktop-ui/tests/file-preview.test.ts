import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { nextTick, type App } from "vue";
import { createPinia, setActivePinia, type Pinia } from "pinia";

import type { FileSide, FileStatus, ReceiveFileContents } from "@/api/types";

const backend = vi.hoisted(() => ({ readReceiveFile: vi.fn(), applyReceive: vi.fn() }));

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

function side(text: string, sha256 = `sha of ${text}`, utf8 = true): FileSide {
  return { kind: "text", text, bom: false, size: text.length, sha256, utf8 };
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
  backend.applyReceive.mockReset();
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

    expect(findAll(root(), "tr").filter((row) => row.props.class !== "chunk-row")).toHaveLength(rowLimit + 11);
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

  it("saves the divider position from the keyboard and from a drag", async () => {
    const project = makeProject(source, "web", [file("a.txt", "added")]);
    backend.readReceiveFile.mockResolvedValue({ local: null, received: side("hello\n") });
    const ui = useUiStore();
    ui.receiveSplit = 32;
    ui.receiveLayout = "tree";
    useTransferStore().setPreview(source, "a.txt");
    mounted = mount(ReceiveProjectTab, install, { project });
    await vi.waitFor(() => expect(findAll(root(), "section")).toHaveLength(1));
    const all = (node: TestNode): TestNode[] => [node, ...node.children.flatMap(all)];
    const grip = all(root()).find((node) => node.props.role === "separator");
    const pane = all(root()).find((node) =>
      String(node.props.class ?? "")
        .split(" ")
        .includes("split"),
    );
    if (!grip || !pane) throw new Error("no divider");
    Object.assign(pane, { getBoundingClientRect: () => ({ left: 0, width: 1200 }) });
    Object.assign(grip, { setPointerCapture: vi.fn() });

    dispatch(grip, "keydown", { key: "ArrowRight" });
    expect(ui.receiveSplit).toBe(34);

    dispatch(grip, "pointerdown", { button: 0, pointerId: 1, clientX: 400 });
    dispatch(grip, "pointermove", { pointerId: 1, clientX: 607 });
    expect(ui.receiveSplit).toBe(50);
  });
});

describe("FilePreview chunks", () => {
  const before = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n";
  const after = "a\nB\nc\nd\ne\nf\ng\nh\nI\nj\n";

  function pills(): TestNode[] {
    return findAll(root(), "button").filter((node) => String(node.props.class ?? "").includes("chunk-pill"));
  }

  function pillStates(): string[] {
    const shown = pills();
    return shown.map((node, index) => {
      const label = textOf(node).trim();
      expect(node.props["aria-pressed"]).toBeUndefined();
      expect(node.props["aria-label"]).toBe(`Chunk ${index + 1} of ${shown.length}: ${label}`);
      return label;
    });
  }

  function switchInput(): TestNode {
    const found = findAll(root(), "input").find((node) => node.props.type === "checkbox");
    if (!found) throw new Error("no switch");
    return found;
  }

  async function shown(relative: string, status: FileStatus, contents: ReceiveFileContents): Promise<void> {
    const project = makeProject(source, "web", [file(relative, status)]);
    const transfer = useTransferStore();
    transfer.plan = { projects: [project] };
    transfer.setFiles(source, [relative], status === "replaced");
    showPreview(relative, status, contents);
    await vi.waitFor(() => expect(textOf(root())).not.toContain("Loading"));
  }

  it("puts a Take pill on each chunk of a replaced file and keeps one when it is clicked", async () => {
    await shown("a.txt", "replaced", { local: side(before, "L"), received: side(after, "R") });
    const transfer = useTransferStore();

    expect(pillStates()).toEqual(["Take", "Take"]);

    click(pills()[1] as TestNode);
    await nextTick();

    expect(pillStates()).toEqual(["Take", "Keep mine"]);
    expect(transfer.fileState(source, "a.txt")).toBe("indeterminate");
    expect(transfer.chunkChoiceFor(source, "a.txt")).toMatchObject({
      receivedSha256: "R",
      localSha256: "L",
      chunks: [
        { oldStart: 1, oldCount: 1, newStart: 1, newCount: 1 },
        { oldStart: 8, oldCount: 1, newStart: 8, newCount: 1 },
      ],
    });
  });

  it("ignores a choice made on other file contents after the file is read again", async () => {
    await shown("a.txt", "replaced", { local: side(before, "L1"), received: side(after, "R1") });
    const transfer = useTransferStore();
    click(pills()[1] as TestNode);
    await nextTick();
    expect([...(transfer.chunkChoiceFor(source, "a.txt")?.taken ?? [])]).toEqual([0]);
    mounted?.unmount();

    for (let index = 0; index < 20; index += 1) await transfer.readFile("C:\\workspace", source, `other${index}.txt`);
    const changed = "A\nb\nc\nD\ne\nf\ng\nh\ni\nJ\n";
    showPreview("a.txt", "replaced", { local: side(before, "L1"), received: side(changed, "R2") });
    await vi.waitFor(() => expect(pills()).toHaveLength(3));
    expect(pillStates()).toEqual(["Take", "Take", "Take"]);

    click(pills()[2] as TestNode);
    await nextTick();
    expect(pillStates()).toEqual(["Take", "Take", "Keep mine"]);

    backend.applyReceive.mockResolvedValue({ projects: [], files: 0, bytes: 0, receivedAt: "", logFile: null });
    await transfer.apply("C:\\workspace", source);
    expect(backend.applyReceive.mock.calls[0]?.[1]?.[0]?.merges).toEqual([
      {
        relative: "a.txt",
        receivedSha256: "R2",
        localSha256: "L1",
        total: 3,
        chunks: [
          { oldStart: 0, oldCount: 1, newStart: 0, newCount: 1 },
          { oldStart: 3, oldCount: 1, newStart: 3, newCount: 1 },
        ],
      },
    ]);
  });

  it("keeps and takes every chunk with Keep all and Take all", async () => {
    await shown("a.txt", "replaced", { local: side(before), received: side(after) });
    const transfer = useTransferStore();

    click(button("Keep all"));
    await nextTick();
    expect(pillStates()).toEqual(["Keep mine", "Keep mine"]);
    expect(transfer.fileState(source, "a.txt")).toBe("unchecked");

    click(button("Take all"));
    await nextTick();
    expect(pillStates()).toEqual(["Take", "Take"]);
    expect(transfer.fileState(source, "a.txt")).toBe("checked");
  });

  it("moves focus between chunk pills with n and p", async () => {
    await shown("a.txt", "replaced", { local: side(before), received: side(after) });
    const section = findAll(root(), "section")[0] as TestNode;

    dispatch(section, "keydown", { key: "n" });
    expect(focused()).toBe(pills()[0]);
    dispatch(focused() as TestNode, "keydown", { key: "n" });
    expect(focused()).toBe(pills()[1]);
    dispatch(focused() as TestNode, "keydown", { key: "n" });
    expect(focused()).toBe(pills()[1]);
    dispatch(focused() as TestNode, "keydown", { key: "p" });
    expect(focused()).toBe(pills()[0]);
  });

  it("offers only the whole file, with a note, when a side is not valid UTF-8", async () => {
    await shown("a.txt", "replaced", { local: side(before, "L", false), received: side(after) });

    expect(pills()).toHaveLength(0);
    expect(buttonLabelled(root(), "Take all")).toBeUndefined();
    expect(textOf(root())).toContain("not valid UTF-8");
  });

  it("offers only the whole file, with a note, for a binary file", async () => {
    await shown("a.bin", "replaced", {
      local: { kind: "binary", size: 10 },
      received: { kind: "binary", size: 12 },
    });

    expect(pills()).toHaveLength(0);
    expect(textOf(root())).toContain("Binary files are received whole");
  });

  it("has no pills for a new file", async () => {
    await shown("a.txt", "added", { local: null, received: side(after) });

    expect(pills()).toHaveLength(0);
    expect(textOf(root())).not.toContain("received whole");
  });

  it("hides whitespace-only chunks with their pills and keeps them when whitespace changes are hidden", async () => {
    const local = "a\nb c\nd\ne\nf\ng\nh\ni\n";
    const received = "a\nb  c\nd\ne\nf\ng\nH\ni\n";
    await shown("a.txt", "replaced", { local: side(local), received: side(received) });
    const transfer = useTransferStore();
    expect(pillStates()).toEqual(["Take", "Take"]);

    dispatch(switchInput(), "change", { target: { checked: true } });
    await nextTick();

    expect(pillStates()).toEqual(["Take"]);
    expect([...(transfer.chunkChoiceFor(source, "a.txt")?.taken ?? [])]).toEqual([1]);

    dispatch(switchInput(), "change", { target: { checked: false } });
    await nextTick();

    expect(pillStates()).toEqual(["Keep mine", "Take"]);
  });

  it("shows Mixed on a pill over a taken and a kept chunk while whitespace is hidden, and takes both on click", async () => {
    await shown("a.txt", "replaced", { local: side("a\n"), received: side("b\na\nb\na \n") });
    const transfer = useTransferStore();
    expect(pillStates()).toEqual(["Take", "Take"]);
    click(pills()[1] as TestNode);
    await nextTick();
    expect(pillStates()).toEqual(["Take", "Keep mine"]);

    dispatch(switchInput(), "change", { target: { checked: true } });
    await nextTick();
    expect(pillStates()).toEqual(["Mixed"]);
    expect([...(transfer.chunkChoiceFor(source, "a.txt")?.taken ?? [])]).toEqual([0]);

    click(pills()[0] as TestNode);
    await nextTick();

    expect(pillStates()).toEqual(["Take"]);
    expect(transfer.fileState(source, "a.txt")).toBe("checked");
    expect(transfer.chunkChoiceFor(source, "a.txt")).toBeNull();
  });

  it("offers only the whole file, with a note, when the exact diff gave up", async () => {
    const before = numbered(1200);
    const after = before.map((line, index) => (index % 2 === 0 ? `${line} changed` : line));
    await shown("a.txt", "replaced", { local: side(text(before)), received: side(text(after)) });

    expect(pills()).toHaveLength(0);
    expect(buttonLabelled(root(), "Take all")).toBeUndefined();
    expect(textOf(root())).toContain("Too many changes to pick them one by one");
  });

  it("takes a whitespace-only file whole with Take all while whitespace is hidden", async () => {
    await shown("a.txt", "whitespace", { local: side("a\nb c\n"), received: side("a\nb  c\n") });

    click(button("Take all"));
    await nextTick();

    expect(useTransferStore().fileState(source, "a.txt")).toBe("checked");
  });

  it("opens a whitespace-only file with whitespace hidden and no pills", async () => {
    await shown("a.txt", "whitespace", { local: side("a\nb c\n"), received: side("a\nb  c\n") });

    expect(pills()).toHaveLength(0);

    dispatch(switchInput(), "change", { target: { checked: false } });
    await nextTick();

    expect(pillStates()).toEqual(["Keep mine"]);
  });
});
