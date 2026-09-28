import { beforeEach, describe, expect, it, vi } from "vitest";
import { createSSRApp, h, type App } from "vue";
import { renderToString } from "vue/server-renderer";
import { createPinia, setActivePinia, type Pinia } from "pinia";

import type { ClipboardContents, ClipboardDownload, ClipboardEntry, ExtractProgress } from "@/api/types";

const backend = vi.hoisted(() => ({
  inspectClipboard: vi.fn(),
  downloadClipboard: vi.fn(),
  cancelClipboardDownload: vi.fn(),
  analyzeReceive: vi.fn(),
}));

vi.mock("@/api/commands", () => ({ ...backend, reportError: () => undefined }));
vi.mock("vue-router", () => ({ useRouter: () => ({ push: vi.fn() }) }));
vi.mock("@/router", () => ({ routeNames: { settings: "settings" } }));
vi.mock("@/components/transfer/DeleteConfirmDialog.vue", () => ({ default: { render: () => null } }));

import ReceivePanel from "@/components/transfer/ReceivePanel.vue";
import { i18n } from "@/i18n";
import { useTransferStore } from "@/stores/transfer";
import { buttonLabelled, click, mount, textOf, type Mounted, type TestNode } from "./helpers/test-renderer";

const root = "C:\\workspace";

let pinia: Pinia;

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
  pinia = createPinia();
  setActivePinia(pinia);
  i18n.global.locale.value = "en";
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
