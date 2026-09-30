import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createSSRApp, h, nextTick, type App } from "vue";
import { renderToString } from "vue/server-renderer";
import { createPinia, setActivePinia, type Pinia } from "pinia";

import type {
  ClipboardContents,
  ClipboardDownload,
  ClipboardEntry,
  ExtractProgress,
  ReceiveSelection,
} from "@/api/types";
import { file, makeProject } from "./helpers/receive-fixtures";

const backend = vi.hoisted(() => ({
  inspectClipboard: vi.fn(),
  downloadClipboard: vi.fn(),
  cancelClipboardDownload: vi.fn(),
  analyzeReceive: vi.fn(),
  applyReceive: vi.fn(),
}));

vi.mock("@/api/commands", () => ({ ...backend, reportError: () => undefined }));
vi.mock("vue-router", () => ({ useRouter: () => ({ push: vi.fn() }) }));
vi.mock("@/router", () => ({ routeNames: { settings: "settings" } }));
vi.mock("@/components/transfer/DeleteConfirmDialog.vue", () => ({ default: { render: () => null } }));
vi.mock("@/components/transfer/DiscardSessionDialog.vue", async () => {
  const { defineComponent, h: render } = await import("vue");
  return {
    default: defineComponent({
      props: { open: Boolean, count: { type: Number, default: 0 } },
      emits: ["cancel", "confirm"],
      setup(props, { emit }) {
        return () =>
          props.open
            ? render("div", [
                render("span", `discard dialog, ${props.count} pending`),
                render("button", { onClick: () => emit("cancel") }, "Keep"),
                render("button", { onClick: () => emit("confirm") }, "Discard"),
              ])
            : null;
      },
    }),
  };
});

import ReceivePanel from "@/components/transfer/ReceivePanel.vue";
import { i18n } from "@/i18n";
import { useTransferStore } from "@/stores/transfer";
import { useUiStore } from "@/stores/ui";
import {
  buttonLabelled,
  click,
  dispatch,
  findAll,
  mount,
  testDocument,
  textOf,
  type Mounted,
  type TestNode,
} from "./helpers/test-renderer";

const root = "C:\\workspace";

let pinia: Pinia;

let keyHandlers: ((event: KeyboardEvent) => void)[] = [];

const fakeWindow = {
  addEventListener: (type: string, handler: (event: KeyboardEvent) => void) => {
    if (type === "keydown") keyHandlers.push(handler);
  },
  removeEventListener: (type: string, handler: (event: KeyboardEvent) => void) => {
    if (type === "keydown") keyHandlers = keyHandlers.filter((entry) => entry !== handler);
  },
};

function press(key: string, altKey = false): void {
  const event = { key, altKey, shiftKey: false, ctrlKey: false, metaKey: false, target: null, preventDefault() {} };
  for (const handler of [...keyHandlers]) handler(event as unknown as KeyboardEvent);
}

function remoteEntry(name: string, files: number, bytes: number | null): ClipboardEntry {
  return {
    path: `virtual:7/${name}`,
    name,
    isProject: true,
    packageName: null,
    suggestedProject: null,
    files,
    bytes,
  };
}

function remote(rejected = 0): ClipboardContents {
  return {
    source: "virtual",
    sequence: 7,
    entries: [remoteEntry("web", 3, 300), remoteEntry("api", 1, null)],
    rejected,
    problem: null,
  };
}

const downloaded: ClipboardDownload = {
  sequence: 7,
  directory: "C:\\recv\\7",
  folders: [
    { id: "virtual:7/web", path: "C:\\recv\\7\\web" },
    { id: "virtual:7/api", path: "C:\\recv\\7\\api" },
  ],
};

function progress(bytesTotal: number | null): ExtractProgress {
  return { filesDone: 1, filesTotal: 4, bytesDone: 100, bytesTotal };
}

async function poll(contents: ClipboardContents): Promise<ReturnType<typeof useTransferStore>> {
  const store = useTransferStore();
  backend.inspectClipboard.mockClear();
  backend.inspectClipboard.mockResolvedValue(contents);
  store.startWatching(root);
  await vi.waitFor(() => expect(backend.inspectClipboard).toHaveBeenCalled());
  await Promise.resolve();
  store.stopWatching();
  return store;
}

async function startDownload(total: number | null, reportsProgress = true): Promise<() => Promise<void>> {
  const store = await poll(remote());
  let finish: () => void = () => undefined;
  backend.downloadClipboard.mockImplementation((_sequence: number, onProgress: (value: ExtractProgress) => void) => {
    if (reportsProgress) onProgress(progress(total));
    return new Promise<ClipboardDownload>((resolve) => {
      finish = () => resolve(downloaded);
    });
  });
  store.setTarget("virtual:7/web", "Web");
  const analyzing = store.analyze(root);
  await vi.waitFor(() => expect(backend.downloadClipboard).toHaveBeenCalled());
  return async () => {
    finish();
    await analyzing;
  };
}

function install(app: App): void {
  app.use(pinia);
  app.use(i18n);
  app.directive("ripple", {});
}

async function render(): Promise<string> {
  const app = createSSRApp({ render: () => h(ReceivePanel) });
  install(app);
  return renderToString(app);
}

function cancelButton(mounted: Mounted): TestNode {
  const found = buttonLabelled(mounted.root, "Cancel");
  if (!found) throw new Error("no Cancel button");
  return found;
}

function text(html: string): string {
  return html
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/<[^>]+>/g, " ")
    .replace(/\s+/g, " ");
}

function tags(html: string, name: string): string[] {
  return html.match(new RegExp(`<${name}\\b[^>]*>`, "g")) ?? [];
}

function button(html: string, label: string): string | undefined {
  return [...html.matchAll(/(<button\b[^>]*>)([\s\S]*?)<\/button>/g)].find((match) =>
    text(match[2] ?? "").includes(label),
  )?.[1];
}

beforeEach(() => {
  for (const mock of Object.values(backend)) mock.mockReset();
  backend.cancelClipboardDownload.mockResolvedValue(undefined);
  backend.analyzeReceive.mockResolvedValue({ projects: [] });
  backend.applyReceive.mockImplementation(async (_root: string, selections: ReceiveSelection[]) => ({
    projects: selections.map((selection) => ({
      target: selection.target,
      targetDirectory: `C:\\repos\\${selection.target}`,
      added: selection.files.length,
      replaced: 0,
      deleted: 0,
      recycled: 0,
      bytes: 10,
      files: selection.files,
      deletedFiles: [],
      skipped: [],
      merged: [],
      stale: [],
    })),
    files: 0,
    bytes: 0,
    receivedAt: "2026-09-30T10:00:00Z",
    logFile: null,
  }));
  pinia = createPinia();
  setActivePinia(pinia);
  i18n.global.locale.value = "en";
  keyHandlers = [];
  vi.stubGlobal("window", fakeWindow);
  vi.stubGlobal("document", testDocument);
  useUiStore().receiveLayout = "tree";
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("ReceivePanel source line", () => {
  it("names a file list as the source and shows each folder's path", async () => {
    const entry = { ...remoteEntry("web", 3, 300), path: "C:\\clip\\web", files: null, bytes: null };
    await poll({ source: "paths", sequence: null, entries: [entry], rejected: 0, problem: null });
    const shown = text(await render());

    expect(shown).toContain("Source: file list on the clipboard");
    expect(shown).toContain("C:\\clip\\web");
    expect(shown).not.toMatch(/\d+ files?\b/);
  });

  it("names a remote desktop as the source and shows each folder's files and size, not its id", async () => {
    await poll(remote(2));
    const shown = text(await render());

    expect(shown).toContain("Source: files from a remote desktop, not downloaded yet");
    expect(shown).toContain("2 files skipped because their names are not safe on Windows");
    expect(shown).toContain("web 3 files · 300 B");
    expect(shown).toContain("api 1 file ");
    expect(shown).not.toContain("1 file ·");
    expect(shown).not.toContain("virtual:7/");
  });

  it("names the download folder and shows where each folder landed after the download", async () => {
    const store = await poll(remote());
    backend.downloadClipboard.mockResolvedValue(downloaded);
    backend.analyzeReceive.mockResolvedValue({ projects: [] });
    store.setTarget("virtual:7/web", "Web");
    await store.analyze(root);
    store.setReceiveStep("clipboard");
    const shown = text(await render());

    expect(shown).toContain("Source: downloaded to C:\\recv\\7");
    expect(shown).toContain("C:\\recv\\7\\web");
    expect(shown).toContain("C:\\recv\\7\\api");
    expect(shown).not.toContain("not downloaded yet");
  });

  it("keeps the folders and says the clipboard is busy", async () => {
    await poll(remote());
    await poll({ source: "busy", sequence: null, entries: [], rejected: 0, problem: null });
    const shown = text(await render());

    expect(shown).toContain("The clipboard is busy, trying again");
    expect(shown).toContain("Source: files from a remote desktop");
    expect(shown).toContain("web 3 files");
  });

  it("shows why the clipboard could not be read", async () => {
    await poll({ source: "unreadable", sequence: null, entries: [], rejected: 0, problem: "Access is denied" });
    expect(text(await render())).toContain("Access is denied");

    await poll({ source: "unreadable", sequence: 8, entries: [], rejected: 0, problem: null });
    expect(text(await render())).toContain("The clipboard could not be read");
  });

  it("shows no source line for an empty clipboard", async () => {
    await poll({ source: "empty", sequence: null, entries: [], rejected: 0, problem: null });
    const html = await render();

    expect(text(html)).not.toContain("Source:");
    expect(html).not.toContain("clipboard-source");
  });

  it("says how many files were skipped even when no folder is left", async () => {
    await poll({ source: "empty", sequence: 9, entries: [], rejected: 3, problem: null });
    const shown = text(await render());

    expect(shown).toContain("3 files skipped because their names are not safe on Windows");
    expect(shown).not.toContain("Source:");
  });

  it("says the clipboard is busy when it was empty before", async () => {
    await poll({ source: "empty", sequence: null, entries: [], rejected: 0, problem: null });
    await poll({ source: "busy", sequence: null, entries: [], rejected: 0, problem: null });

    expect(text(await render())).toContain("The clipboard is busy, trying again");
  });
});

describe("ReceivePanel download", () => {
  it("shows the progress with a Cancel button and locks Analyze and the targets", async () => {
    const finish = await startDownload(400);
    const html = await render();
    const shown = text(html);

    expect(shown).toContain("Downloading from the remote desktop");
    expect(shown).toContain("Files: 1 of 4 · 100 B of 400 B");
    expect(button(html, "Cancel")).not.toMatch(/\bdisabled\b/);
    expect(button(html, "Analyze")).toMatch(/\bdisabled\b/);
    expect(tags(html, "select")).toHaveLength(2);
    expect(tags(html, "select").every((tag) => /\bdisabled\b/.test(tag))).toBe(true);
    expect(html).toContain("width:25%");
    expect(html).not.toContain("indeterminate");
    await finish();
  });

  it("shows a moving bar and the bytes so far while the total size is unknown", async () => {
    const finish = await startDownload(null);
    const html = await render();
    const shown = text(html);

    expect(shown).toContain("Files: 1 of 4 · 100 B");
    expect(shown).not.toContain("100 B of");
    expect(tags(html, "div").some((tag) => /class="[^"]*\bindeterminate\b/.test(tag))).toBe(true);
    expect(html).not.toMatch(/width:\d/);
    await finish();
  });

  it("locks the actions at once and shows the progress with Cancel after a short delay when no report came", async () => {
    vi.useFakeTimers();
    try {
      const finish = await startDownload(400, false);
      const early = await render();

      expect(text(early)).not.toContain("Downloading");
      expect(button(early, "Analyze")).toMatch(/\bdisabled\b/);
      expect(tags(early, "select").every((tag) => /\bdisabled\b/.test(tag))).toBe(true);

      vi.advanceTimersByTime(1000);
      const late = await render();

      expect(text(late)).toContain("Downloading from the remote desktop");
      expect(button(late, "Cancel")).not.toMatch(/\bdisabled\b/);
      expect(late).toContain("indeterminate");
      await finish();
    } finally {
      vi.useRealTimers();
    }
  });

  it("asks the backend to cancel when Cancel is clicked", async () => {
    const finish = await startDownload(400);
    const mounted = mount(ReceivePanel, install);
    try {
      click(cancelButton(mounted));

      expect(backend.cancelClipboardDownload).toHaveBeenCalledTimes(1);
    } finally {
      mounted.unmount();
      await finish();
    }
  });

  it("shows the error when the cancel request fails", async () => {
    const finish = await startDownload(400);
    backend.cancelClipboardDownload.mockRejectedValue("The receive state is not available");
    const mounted = mount(ReceivePanel, install);
    try {
      click(cancelButton(mounted));

      await vi.waitFor(() => expect(textOf(mounted.root)).toContain("The receive state is not available"));
    } finally {
      mounted.unmount();
      await finish();
    }
  });

  it("leaves Analyze and the targets usable and hides the progress when nothing is downloading", async () => {
    const store = await poll(remote());
    store.setTarget("virtual:7/web", "Web");
    const html = await render();

    expect(text(html)).not.toContain("Downloading");
    expect(button(html, "Analyze")).not.toMatch(/\bdisabled\b/);
    expect(tags(html, "select").some((tag) => /\bdisabled\b/.test(tag))).toBe(false);
  });
});

function diskEntry(name: string): ClipboardEntry {
  return {
    path: `C:\\clip\\${name}`,
    name,
    isProject: true,
    packageName: null,
    suggestedProject: name,
    files: null,
    bytes: null,
  };
}

function onDisk(names: string[]): ClipboardContents {
  return { source: "paths", sequence: null, entries: names.map(diskEntry), rejected: 0, problem: null };
}

function planFor(names: string[]): { projects: ReturnType<typeof makeProject>[] } {
  return { projects: names.map((name) => makeProject(`C:\\clip\\${name}`, name, [file("a.ts", "added")])) };
}

async function session(names: string[]): Promise<ReturnType<typeof useTransferStore>> {
  const store = await poll(onDisk(names));
  backend.analyzeReceive.mockResolvedValue(planFor(names));
  await store.analyze(root);
  expect(store.receiveStep).toBe("review");
  return store;
}

function exactButton(mounted: Mounted, label: string): TestNode {
  const found = findAll(mounted.root, "button").find((node) => textOf(node).trim() === label);
  if (!found) throw new Error(`no ${label} button`);
  return found;
}

function hasButton(mounted: Mounted, label: string): boolean {
  return findAll(mounted.root, "button").some((node) => textOf(node).trim() === label);
}

describe("ReceivePanel steps", () => {
  it("goes to the review step on Analyze, back with Back or Alt+Left, forward with Alt+Right, never on Enter", async () => {
    const store = await poll(onDisk(["web", "api"]));
    backend.analyzeReceive.mockResolvedValue(planFor(["web", "api"]));
    const mounted = mount(ReceivePanel, install);
    try {
      press("ArrowRight", true);
      expect(store.receiveStep).toBe("clipboard");
      press("Enter");
      expect(backend.analyzeReceive).not.toHaveBeenCalled();

      click(exactButton(mounted, "Analyze"));
      await vi.waitFor(() => expect(textOf(mounted.root)).toContain("Replace selected in all projects"));
      expect(store.receiveStep).toBe("review");

      click(exactButton(mounted, "Back"));
      await vi.waitFor(() => expect(textOf(mounted.root)).toContain("Folders on the clipboard"));
      expect(store.tabs).toHaveLength(2);

      press("ArrowRight", true);
      expect(store.receiveStep).toBe("review");
      press("ArrowLeft", true);
      expect(store.receiveStep).toBe("clipboard");
      press("Enter");
      expect(backend.analyzeReceive).toHaveBeenCalledTimes(1);
    } finally {
      mounted.unmount();
    }
  });

  it("opens the review step from the stepper only while a session exists", async () => {
    const store = await poll(onDisk(["web"]));
    const mounted = mount(ReceivePanel, install);
    try {
      const reviewStep = () => findAll(mounted.root, "li").find((node) => textOf(node).includes("Review & replace"));
      dispatch(reviewStep() as TestNode, "click");
      expect(store.receiveStep).toBe("clipboard");

      backend.analyzeReceive.mockResolvedValue(planFor(["web"]));
      await store.analyze(root);
      store.setReceiveStep("clipboard");
      await nextTick();
      dispatch(reviewStep() as TestNode, "click");
      expect(store.receiveStep).toBe("review");
    } finally {
      mounted.unmount();
    }
  });

  it("analyzes the same clipboard again without asking and keeps the results", async () => {
    const store = await session(["web", "api"]);
    await store.apply(root, "C:\\clip\\web");
    store.setReceiveStep("clipboard");
    const mounted = mount(ReceivePanel, install);
    try {
      click(exactButton(mounted, "Analyze"));
      await vi.waitFor(() => expect(backend.analyzeReceive).toHaveBeenCalledTimes(2));
      expect(textOf(mounted.root)).not.toContain("discard dialog");
      await vi.waitFor(() => expect(store.receiveStep).toBe("review"));
      expect(store.results["C:\\clip\\web"]?.result.target).toBe("web");
    } finally {
      mounted.unmount();
    }
  });

  it("asks before discarding a session with projects not replaced yet, and analyzes after the confirm", async () => {
    const store = await session(["web", "api"]);
    await store.apply(root, "C:\\clip\\web");
    await poll(onDisk(["web", "docs"]));
    store.setReceiveStep("clipboard");
    const mounted = mount(ReceivePanel, install);
    try {
      click(exactButton(mounted, "Analyze"));
      await vi.waitFor(() => expect(textOf(mounted.root)).toContain("discard dialog, 1 pending"));
      expect(backend.analyzeReceive).toHaveBeenCalledTimes(1);

      click(exactButton(mounted, "Keep"));
      await vi.waitFor(() => expect(textOf(mounted.root)).not.toContain("discard dialog"));
      expect(backend.analyzeReceive).toHaveBeenCalledTimes(1);
      expect(store.tabs.map((project) => project.target)).toEqual(["web", "api"]);

      backend.analyzeReceive.mockResolvedValue(planFor(["web", "docs"]));
      click(exactButton(mounted, "Analyze"));
      await vi.waitFor(() => expect(textOf(mounted.root)).toContain("discard dialog"));
      click(exactButton(mounted, "Discard"));
      await vi.waitFor(() => expect(store.tabs.map((project) => project.target)).toEqual(["web", "docs"]));
      expect(backend.analyzeReceive).toHaveBeenCalledTimes(2);
      expect(store.results).toEqual({});
      expect(store.receiveStep).toBe("review");
    } finally {
      mounted.unmount();
    }
  });

  it("analyzes another clipboard without asking when every project is replaced", async () => {
    const store = await session(["web", "api"]);
    await store.apply(root);
    await poll(onDisk(["docs"]));
    store.setReceiveStep("clipboard");
    backend.analyzeReceive.mockResolvedValue(planFor(["docs"]));
    const mounted = mount(ReceivePanel, install);
    try {
      click(exactButton(mounted, "Analyze"));
      await vi.waitFor(() => expect(store.tabs.map((project) => project.target)).toEqual(["docs"]));
      expect(textOf(mounted.root)).not.toContain("discard dialog");
    } finally {
      mounted.unmount();
    }
  });

  it("says on the review step that the clipboard changed, with a way back to the clipboard step", async () => {
    const store = await session(["web"]);
    const mounted = mount(ReceivePanel, install);
    try {
      expect(textOf(mounted.root)).not.toContain("The clipboard has changed.");
      backend.inspectClipboard.mockResolvedValue(onDisk(["docs"]));
      store.stopWatching();
      store.startWatching(root);
      await vi.waitFor(() => expect(textOf(mounted.root)).toContain("The clipboard has changed."));
      expect(store.tabs.map((project) => project.target)).toEqual(["web"]);

      click(exactButton(mounted, "Back to Clipboard"));
      expect(store.receiveStep).toBe("clipboard");
      expect(store.hasSession).toBe(true);
    } finally {
      mounted.unmount();
    }
  });

  it("offers Receive more once every project is replaced or closed, which ends the session", async () => {
    const store = await session(["web", "api"]);
    const mounted = mount(ReceivePanel, install);
    try {
      expect(hasButton(mounted, "Receive more")).toBe(false);
      await store.apply(root, "C:\\clip\\web");
      store.closeProject("C:\\clip\\api");
      await vi.waitFor(() => expect(hasButton(mounted, "Receive more")).toBe(true));
      expect(textOf(mounted.root)).toContain("Received into web");

      click(exactButton(mounted, "Receive more"));
      expect(store.hasSession).toBe(false);
      expect(store.receiveStep).toBe("clipboard");
      await vi.waitFor(() => expect(textOf(mounted.root)).toContain("Folders on the clipboard"));
    } finally {
      mounted.unmount();
    }
  });
});
