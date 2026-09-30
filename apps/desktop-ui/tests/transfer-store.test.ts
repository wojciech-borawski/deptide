import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

import type {
  ClipboardContents,
  ClipboardDownload,
  ClipboardEntry,
  ExtractProgress,
  FileSide,
  LineChunk,
  ReceiveFileContents,
  ReceivePlan,
  ReceiveProjectPlan,
  ReceiveProjectResult,
  ReceiveRequest,
  ReceiveResult,
  ReceiveSelection,
} from "@/api/types";
import { file, makeProject } from "./helpers/receive-fixtures";

const backend = vi.hoisted(() => ({
  previewTransfer: vi.fn(),
  copyProjectsToClipboard: vi.fn(),
  inspectClipboard: vi.fn(),
  downloadClipboard: vi.fn(),
  cancelClipboardDownload: vi.fn(),
  analyzeReceive: vi.fn(),
  applyReceive: vi.fn(),
  readReceiveFile: vi.fn(),
}));

vi.mock("@/api/commands", () => ({
  ...backend,
  reportError: () => undefined,
}));

import { useTransferStore } from "@/stores/transfer";

const root = "C:\\workspace";

function resetBackend(): void {
  for (const mock of Object.values(backend)) mock.mockReset();
}

function entry(name: string, suggestedProject: string | null = name): ClipboardEntry {
  return {
    path: `C:\\clip\\${name}`,
    name,
    isProject: true,
    packageName: null,
    suggestedProject,
    files: null,
    bytes: null,
  };
}

function onDisk(entries: ClipboardEntry[]): ClipboardContents {
  return { source: "paths", sequence: null, entries, rejected: 0, problem: null };
}

function remoteId(sequence: number, name: string): string {
  return `virtual:${sequence}/${name}`;
}

function remote(sequence: number, names: string[], rejected = 0): ClipboardContents {
  return {
    source: "virtual",
    sequence,
    entries: names.map((name) => ({
      path: remoteId(sequence, name),
      name,
      isProject: true,
      packageName: null,
      suggestedProject: name,
      files: 3,
      bytes: 300,
    })),
    rejected,
    problem: null,
  };
}

function receivedPath(sequence: number, name: string): string {
  return `C:\\recv\\${sequence}\\${name}`;
}

function downloaded(sequence: number, names: string[]): ClipboardDownload {
  return {
    sequence,
    directory: `C:\\recv\\${sequence}`,
    folders: names.map((name) => ({ id: remoteId(sequence, name), path: receivedPath(sequence, name) })),
  };
}

const busy: ClipboardContents = { source: "busy", sequence: null, entries: [], rejected: 0, problem: null };

const web = makeProject("C:\\clip\\web", "web", [
  file("src/new.ts", "added"),
  file("src/old.ts", "removed"),
  file("src/app.ts", "whitespace"),
  file("src/same.ts", "identical"),
]);
const api = makeProject("C:\\clip\\api", "api", [file("index.ts", "replaced"), file("gone.ts", "removed")]);
const docs = makeProject("C:\\clip\\docs", "docs", [file("README.md", "identical")]);
const plan: ReceivePlan = { projects: [web, api, docs] };

function projectResult(selection: ReceiveSelection): ReceiveProjectResult {
  return {
    target: selection.target,
    targetDirectory: `C:\\repos\\${selection.target}`,
    added: selection.files.length,
    replaced: 0,
    deleted: selection.delete.length,
    recycled: selection.delete.length,
    bytes: 10,
    files: selection.files,
    deletedFiles: selection.delete,
    skipped: [],
    merged: [],
    stale: [],
  };
}

function resultFor(selections: ReceiveSelection[]): ReceiveResult {
  return {
    projects: selections.map(projectResult),
    files: 0,
    bytes: 0,
    receivedAt: "2026-09-28T10:00:00Z",
    logFile: "C:\\workspace\\transfers\\receive.json",
  };
}

async function poll(store: ReturnType<typeof useTransferStore>, contents: ClipboardContents): Promise<void> {
  backend.inspectClipboard.mockClear();
  backend.inspectClipboard.mockResolvedValue(contents);
  store.startWatching(root);
  await vi.waitFor(() => expect(backend.inspectClipboard).toHaveBeenCalled());
  await Promise.resolve();
  store.stopWatching();
}

async function watchClipboard(store: ReturnType<typeof useTransferStore>, entries: ClipboardEntry[]): Promise<void> {
  await poll(store, onDisk(entries));
  expect(store.entries).toHaveLength(entries.length);
}

function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void; reject: (cause: unknown) => void } {
  let resolve: (value: T) => void = () => undefined;
  let reject: (cause: unknown) => void = () => undefined;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}

function relatives(project: ReceiveProjectPlan | undefined): string[] {
  return project?.files.map((entry) => entry.relative) ?? [];
}

async function analyzed(): Promise<ReturnType<typeof useTransferStore>> {
  const store = useTransferStore();
  await watchClipboard(store, [entry("web"), entry("api"), entry("docs")]);
  backend.analyzeReceive.mockResolvedValue(structuredClone(plan));
  await store.analyze(root);
  return store;
}

describe("transfer store", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    resetBackend();
    backend.applyReceive.mockImplementation(async (_root: string, selections: ReceiveSelection[]) =>
      resultFor(selections),
    );
  });

  it("passes the patterns switched off on the Copy tab to preview and copy", async () => {
    const store = useTransferStore();
    backend.previewTransfer.mockResolvedValue({ projects: [], files: 0, bytes: 0, patterns: [] });
    backend.copyProjectsToClipboard.mockResolvedValue({ projects: [], files: 0, bytes: 0 });
    store.setCopyProjects(["web"]);

    store.toggleCopyPattern("*.log");
    store.toggleCopyPattern("coverage/");
    store.toggleCopyPattern("*.log");
    await store.previewCopy(root);
    await store.copy(root);

    expect(backend.previewTransfer).toHaveBeenCalledWith(root, ["web"], [], ["coverage/"]);
    expect(backend.copyProjectsToClipboard).toHaveBeenCalledWith(root, ["web"], [], ["coverage/"]);
    expect(store.receiveDisabled).toEqual([]);
  });

  it("passes the patterns switched off on the Receive tab to analyze, without touching the Copy ones", async () => {
    const store = useTransferStore();
    await watchClipboard(store, [entry("web")]);
    backend.analyzeReceive.mockResolvedValue({ projects: [web] });

    store.toggleCopyPattern("dist/");
    store.toggleReceivePattern("*.log");
    expect(backend.analyzeReceive).not.toHaveBeenCalled();

    await store.analyze(root);

    expect(backend.analyzeReceive).toHaveBeenCalledWith(
      root,
      [{ source: "C:\\clip\\web", target: "web" }],
      [],
      ["*.log"],
    );
  });

  it("does not analyze again when a Receive pattern is toggled while a plan is shown", async () => {
    const store = await analyzed();
    backend.analyzeReceive.mockClear();

    store.toggleReceivePattern(".env.local");

    expect(backend.analyzeReceive).not.toHaveBeenCalled();
    expect(store.receiveDisabled).toEqual([".env.local"]);
    expect(store.tabs.map((project) => project.target)).toEqual(["web", "api"]);

    await store.analyze(root);
    expect(backend.analyzeReceive.mock.calls[0]?.[3]).toEqual([".env.local"]);
  });

  it("opens a tab per changed project, lists the rest as unchanged, and ticks added, replaced and removed files", async () => {
    const store = await analyzed();

    expect(store.tabs.map((project) => project.target)).toEqual(["web", "api"]);
    expect(store.unchangedProjects.map((project) => project.target)).toEqual(["docs"]);
    expect(store.activeSource).toBe(web.source);
    expect([...store.selectedFor(web.source)].sort()).toEqual(["src/new.ts", "src/old.ts"]);
    expect(store.selectedCount).toBe(4);
  });

  it("applies one project, keeps the other tabs as they were and stores the result", async () => {
    const store = await analyzed();

    await store.apply(root, web.source);

    expect(backend.applyReceive).toHaveBeenCalledWith(root, [
      { source: web.source, target: "web", files: ["src/new.ts"], delete: ["src/old.ts"] },
    ]);
    expect(store.results[web.source]?.result.deletedFiles).toEqual(["src/old.ts"]);
    expect(store.results[web.source]?.logFile).toBe("C:\\workspace\\transfers\\receive.json");
    expect(store.results[api.source]).toBeUndefined();
    expect(store.tabs.map((project) => project.target)).toEqual(["web", "api"]);
    expect([...store.selectedFor(api.source)].sort()).toEqual(["gone.ts", "index.ts"]);
    expect(store.selectedCount).toBe(2);
  });

  it("applies every open tab that has no result yet and never the unchanged projects", async () => {
    const store = await analyzed();
    await store.apply(root, web.source);
    backend.applyReceive.mockClear();
    store.toggleFile(docs.source, "README.md");

    await store.apply(root);

    expect(backend.applyReceive).toHaveBeenCalledWith(root, [
      { source: api.source, target: "api", files: ["index.ts"], delete: ["gone.ts"] },
    ]);
    expect(store.results[api.source]?.result.target).toBe("api");
  });

  it("lists the ticked removed files of the projects a replace would touch", async () => {
    const store = await analyzed();

    expect(store.removalsFor(api.source)).toEqual([{ source: api.source, target: "api", files: ["gone.ts"] }]);
    expect(store.removalsFor().map((group) => group.target)).toEqual(["web", "api"]);

    store.toggleFile(web.source, "src/old.ts");
    expect(store.removalsFor().map((group) => group.target)).toEqual(["api"]);
  });

  it("closing a tab stops receiving that folder and drops its plan", async () => {
    const store = await analyzed();

    store.closeProject(web.source);

    expect(store.targets[web.source]).toBe("");
    expect(store.tabs.map((project) => project.target)).toEqual(["api"]);
    expect(store.activeSource).toBe(api.source);
    expect(store.requests.map((request) => request.target)).toEqual(["api", "docs"]);
  });

  it("remembers the previewed file per tab and forgets it when the plan changes", async () => {
    const store = await analyzed();

    store.setPreview(web.source, "src/new.ts");
    store.setPreview(api.source, "index.ts");
    store.setPreview(api.source, null);

    expect(store.previewFor(web.source)).toBe("src/new.ts");
    expect(store.previewFor(api.source)).toBeNull();

    await store.analyze(root);
    expect(store.previewFor(web.source)).toBeNull();
  });

  it("reads a previewed file once per plan and again after the next analyze", async () => {
    const store = await analyzed();
    const contents = { received: { kind: "text", text: "a", bom: false, size: 1 }, local: null };
    backend.readReceiveFile.mockResolvedValue(contents);

    const first = await store.readFile(root, web.source, "src/new.ts");
    const second = await store.readFile(root, web.source, "src/new.ts");
    await store.readFile(root, api.source, "index.ts");

    expect(first).toEqual(contents);
    expect(second).toBe(first);
    expect(backend.readReceiveFile).toHaveBeenCalledTimes(2);
    expect(backend.readReceiveFile).toHaveBeenCalledWith(root, web.source, "web", "src/new.ts");

    await store.analyze(root);
    await store.readFile(root, web.source, "src/new.ts");
    expect(backend.readReceiveFile).toHaveBeenCalledTimes(3);
  });

  it("keeps the contents of the same path in two projects apart", async () => {
    const store = await analyzed();
    backend.readReceiveFile.mockImplementation(async (_root: string, source: string) => ({
      received: { kind: "text", text: source, bom: false, size: source.length },
      local: null,
    }));

    const fromWeb = await store.readFile(root, web.source, "package.json");
    const fromApi = await store.readFile(root, api.source, "package.json");

    expect(fromWeb.received).toMatchObject({ text: web.source });
    expect(fromApi.received).toMatchObject({ text: api.source });
    expect(backend.readReceiveFile).toHaveBeenCalledTimes(2);
  });

  it("keeps the 20 most recently used files and reads an evicted one again", async () => {
    const store = await analyzed();
    backend.readReceiveFile.mockResolvedValue({ received: null, local: null });
    const paths = Array.from({ length: 21 }, (_, index) => `src/f${index}.ts`);

    for (const path of paths.slice(0, 20)) await store.readFile(root, web.source, path);
    await store.readFile(root, web.source, "src/f0.ts");
    await store.readFile(root, web.source, "src/f20.ts");
    expect(backend.readReceiveFile).toHaveBeenCalledTimes(21);

    await store.readFile(root, web.source, "src/f0.ts");
    expect(backend.readReceiveFile).toHaveBeenCalledTimes(21);
    await store.readFile(root, web.source, "src/f1.ts");
    expect(backend.readReceiveFile).toHaveBeenCalledTimes(22);
    expect(backend.readReceiveFile).toHaveBeenLastCalledWith(root, web.source, "web", "src/f1.ts");
  });

  it("reads a file again after a failed read", async () => {
    const store = await analyzed();
    backend.readReceiveFile.mockRejectedValueOnce(new Error("locked"));
    backend.readReceiveFile.mockResolvedValueOnce({ received: null, local: null });

    await expect(store.readFile(root, web.source, "src/old.ts")).rejects.toThrow("locked");
    await expect(store.readFile(root, web.source, "src/old.ts")).resolves.toEqual({ received: null, local: null });
    expect(backend.readReceiveFile).toHaveBeenCalledTimes(2);
  });

  it("stores each result of a global replace under its own project", async () => {
    const store = await analyzed();

    await store.apply(root);

    expect(backend.applyReceive).toHaveBeenCalledWith(root, [
      { source: web.source, target: "web", files: ["src/new.ts"], delete: ["src/old.ts"] },
      { source: api.source, target: "api", files: ["index.ts"], delete: ["gone.ts"] },
    ]);
    expect(store.results[web.source]?.result.target).toBe("web");
    expect(store.results[api.source]?.result.target).toBe("api");
    expect(store.selectedCount).toBe(0);
  });

  it("ticks only the visible files for All while identical files are hidden, and every file otherwise", async () => {
    const store = await analyzed();

    store.selectFiles(web.source, "all", true);
    expect([...store.selectedFor(web.source)].sort()).toEqual(["src/app.ts", "src/new.ts", "src/old.ts"]);
    expect(store.countFor(web.source)).toBe(3);

    store.selectFiles(web.source, "all", false);
    expect([...store.selectedFor(web.source)].sort()).toEqual([
      "src/app.ts",
      "src/new.ts",
      "src/old.ts",
      "src/same.ts",
    ]);
    expect(store.countFor(web.source)).toBe(4);
  });

  it("unticks identical files when they are hidden, so the replace only sends visible files", async () => {
    const store = await analyzed();
    store.selectFiles(web.source, "all", false);

    store.untickIdentical();

    expect([...store.selectedFor(web.source)].sort()).toEqual(["src/app.ts", "src/new.ts", "src/old.ts"]);
    expect([...store.selectedFor(api.source)].sort()).toEqual(["gone.ts", "index.ts"]);
    expect(store.countFor(web.source)).toBe(3);

    await store.apply(root, web.source);

    expect(backend.applyReceive).toHaveBeenCalledWith(root, [
      { source: web.source, target: "web", files: ["src/new.ts", "src/app.ts"], delete: ["src/old.ts"] },
    ]);
  });

  it("clears the Copy preview when a pattern is toggled", async () => {
    const store = useTransferStore();
    backend.previewTransfer.mockResolvedValue({ projects: [], files: 3, bytes: 30, patterns: [] });
    store.setCopyProjects(["web"]);
    await store.previewCopy(root);
    expect(store.preview?.files).toBe(3);

    store.toggleCopyPattern("*.log");

    expect(store.preview).toBeNull();
  });

  it("shows the plan of the latest analyze when the earlier one answers last", async () => {
    const store = await analyzed();
    const first = deferred<ReceivePlan>();
    const second = deferred<ReceivePlan>();
    backend.analyzeReceive.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);

    store.toggleReceivePattern("*.log");
    const firstAnalyze = store.analyze(root);
    store.toggleReceivePattern("*.map");
    const secondAnalyze = store.analyze(root);
    second.resolve({ projects: [makeProject(web.source, "web", [file("src/second.ts", "added")])] });
    await secondAnalyze;
    first.resolve({ projects: [makeProject(web.source, "web", [file("src/first.ts", "added")])] });
    await firstAnalyze;

    expect(backend.analyzeReceive.mock.calls.at(-1)?.[3]).toEqual(["*.log", "*.map"]);
    expect(store.tabs.map(relatives)).toEqual([["src/second.ts"]]);
    expect([...store.selectedFor(web.source)]).toEqual(["src/second.ts"]);
  });

  it("does not show the error of an analyze that a newer one replaced", async () => {
    const store = await analyzed();
    const first = deferred<ReceivePlan>();
    backend.analyzeReceive
      .mockReturnValueOnce(first.promise)
      .mockResolvedValueOnce({ projects: [makeProject(web.source, "web", [file("src/second.ts", "added")])] });

    const firstAnalyze = store.analyze(root);
    await store.analyze(root);
    first.reject(new Error("stale"));
    await firstAnalyze;

    expect(store.receiveError).toBe("");
    expect(store.tabs.map(relatives)).toEqual([["src/second.ts"]]);
  });

  it("does not bring an ended session back when its Analyze answers late", async () => {
    const store = useTransferStore();
    await watchClipboard(store, [entry("web")]);
    const pending = deferred<ReceivePlan>();
    backend.analyzeReceive.mockReturnValueOnce(pending.promise);

    const running = store.analyze(root);
    store.endSession();
    pending.resolve({ projects: [web] });
    await running;

    expect(store.hasSession).toBe(false);
    expect(store.plan).toBeNull();
    expect(store.receiveStep).toBe("clipboard");
  });

  it("does not bring an ended session back when the plan refresh after a failed replace answers late", async () => {
    const store = await analyzed();
    backend.applyReceive.mockRejectedValue(new Error("api: access denied"));
    const refreshing = deferred<ReceivePlan>();
    backend.analyzeReceive.mockReturnValueOnce(refreshing.promise);

    const running = store.apply(root, api.source);
    await vi.waitFor(() => expect(backend.analyzeReceive).toHaveBeenCalledTimes(2));
    store.endSession();
    refreshing.resolve(structuredClone(plan));
    await running;

    expect(store.hasSession).toBe(false);
    expect(store.plan).toBeNull();
    expect(store.receiveStep).toBe("clipboard");
  });

  it("keeps the plan of a newer Analyze when the refresh after a failed replace answers late", async () => {
    const store = await analyzed();
    backend.applyReceive.mockRejectedValue(new Error("api: access denied"));
    const refreshing = deferred<ReceivePlan>();
    backend.analyzeReceive.mockReturnValueOnce(refreshing.promise);

    const running = store.apply(root, api.source);
    await vi.waitFor(() => expect(backend.analyzeReceive).toHaveBeenCalledTimes(2));
    backend.analyzeReceive.mockResolvedValueOnce({
      projects: [makeProject(web.source, "web", [file("src/newer.ts", "added")])],
    });
    await store.analyze(root);
    refreshing.resolve(structuredClone(plan));
    await running;

    expect(store.tabs.map(relatives)).toEqual([["src/newer.ts"]]);
  });

  it("keeps applied results and manual ticks when Analyze runs again on the same clipboard", async () => {
    const store = await analyzed();
    await store.apply(root, web.source);
    store.toggleFile(api.source, "gone.ts");
    store.setActive(api.source);
    const webAfterApply = makeProject(web.source, "web", [
      file("src/new.ts", "identical"),
      file("src/same.ts", "identical"),
    ]);
    const apiAgain = makeProject(api.source, "api", [
      file("index.ts", "replaced"),
      file("gone.ts", "removed"),
      file("extra.ts", "added"),
    ]);
    backend.analyzeReceive.mockResolvedValue({ projects: [webAfterApply, apiAgain, docs] });

    store.toggleReceivePattern("*.log");
    await store.analyze(root);

    expect(store.results[web.source]?.result.files).toEqual(["src/new.ts"]);
    expect(store.tabs.map((project) => project.target)).toEqual(["web", "api"]);
    expect(relatives(store.tabs[1])).toEqual(["index.ts", "gone.ts", "extra.ts"]);
    expect([...store.selectedFor(api.source)].sort()).toEqual(["extra.ts", "index.ts"]);
    expect(store.activeSource).toBe(api.source);
  });

  it("analyzes the shown plan again after a failed replace and keeps the error visible", async () => {
    const store = await analyzed();
    store.toggleReceivePattern("*.log");
    await store.analyze(root);
    store.toggleReceivePattern("*.log");
    store.receivePatterns = "*.tmp";
    const webApplied = makeProject(web.source, "web", [
      file("src/new.ts", "identical"),
      file("src/app.ts", "identical"),
      file("src/same.ts", "identical"),
    ]);
    backend.analyzeReceive.mockClear();
    backend.analyzeReceive.mockResolvedValue({ projects: [webApplied, api, docs] });
    backend.applyReceive.mockRejectedValue(new Error("api: access denied"));

    await store.apply(root);

    expect(backend.analyzeReceive).toHaveBeenCalledTimes(1);
    expect(backend.analyzeReceive).toHaveBeenCalledWith(
      root,
      [
        { source: web.source, target: "web" },
        { source: api.source, target: "api" },
        { source: docs.source, target: "docs" },
      ],
      [],
      ["*.log"],
    );
    expect(store.receiveError).toBe("Error: api: access denied");
    expect(store.tabs.map((project) => project.target)).toEqual(["api"]);
    expect(store.unchangedProjects.map((project) => project.target)).toEqual(["web", "docs"]);
    expect(store.results[api.source]).toBeUndefined();
    expect(store.receiveBusy).toBe(false);
  });

  it("keeps the plan, ticks and results when a poll finds the same clipboard entries", async () => {
    const store = await analyzed();
    await store.apply(root, web.source);
    store.toggleFile(api.source, "gone.ts");
    backend.inspectClipboard.mockClear();

    store.startWatching(root);
    await vi.waitFor(() => expect(backend.inspectClipboard).toHaveBeenCalledTimes(1));
    await new Promise((done) => setTimeout(done, 0));
    store.stopWatching();

    expect(store.tabs.map((project) => project.target)).toEqual(["web", "api"]);
    expect([...store.selectedFor(api.source)]).toEqual(["index.ts"]);
    expect(store.results[web.source]?.result.target).toBe("web");
  });

  it("keeps an analyze answer that arrives after the clipboard changed, as the session of the analyzed clipboard", async () => {
    const store = useTransferStore();
    await watchClipboard(store, [entry("web"), entry("api"), entry("docs")]);
    const pending = deferred<ReceivePlan>();
    backend.analyzeReceive.mockReturnValueOnce(pending.promise);

    const running = store.analyze(root);
    await watchClipboard(store, [entry("other")]);
    pending.resolve(structuredClone(plan));
    await running;

    expect(store.tabs.map((project) => project.target)).toEqual(["web", "api"]);
    expect(store.clipboardChanged).toBe(true);
    expect(store.receiveStep).toBe("review");
    expect(store.entries.map((found) => found.name)).toEqual(["other"]);
  });

  it("keeps the plan, ticks and results when the clipboard changes to text or to other folders", async () => {
    const store = await analyzed();
    await store.apply(root, web.source);
    store.toggleFile(api.source, "gone.ts");
    expect(store.clipboardChanged).toBe(false);

    await poll(store, { source: "empty", sequence: 12, entries: [], rejected: 0, problem: null });

    expect(store.clipboardChanged).toBe(true);
    expect(store.entries).toEqual([]);
    expect(store.tabs.map((project) => project.target)).toEqual(["web", "api"]);
    expect([...store.selectedFor(api.source)]).toEqual(["index.ts"]);
    expect(store.results[web.source]?.result.target).toBe("web");

    await watchClipboard(store, [entry("other")]);

    expect(store.clipboardChanged).toBe(true);
    expect({ ...store.targets }).toEqual({ "C:\\clip\\other": "other" });
    expect(store.tabs.map((project) => project.target)).toEqual(["web", "api"]);
    expect(store.results[web.source]?.result.target).toBe("web");
    expect([...store.selectedFor(api.source)]).toEqual(["index.ts"]);
    expect(store.pendingCount).toBe(1);

    await watchClipboard(store, [entry("web"), entry("api"), entry("docs")]);
    expect(store.clipboardChanged).toBe(false);
  });

  it("starts a new session when Analyze runs on another clipboard", async () => {
    const store = await analyzed();
    await store.apply(root, web.source);
    await watchClipboard(store, [entry("other")]);
    const other = makeProject("C:\\clip\\other", "other", [file("x.ts", "added")]);
    backend.analyzeReceive.mockResolvedValue({ projects: [other] });

    await store.analyze(root);

    expect(backend.analyzeReceive).toHaveBeenLastCalledWith(root, [{ source: other.source, target: "other" }], [], []);
    expect(store.tabs.map((project) => project.target)).toEqual(["other"]);
    expect(store.results).toEqual({});
    expect(store.clipboardChanged).toBe(false);
    expect(store.activeSource).toBe(other.source);
  });

  it("drops the results, ticks and chunk choices of projects also in the plan of another clipboard", async () => {
    const store = await analyzed();
    await store.apply(root, web.source);
    store.toggleFile(api.source, "gone.ts");
    store.setChunks(api.source, "index.ts", indexChunks, [1], false);
    backend.readReceiveFile.mockResolvedValue(contents("l1", "r1"));
    await watchClipboard(store, [entry("web"), entry("api")]);
    backend.analyzeReceive.mockResolvedValue({ projects: [web, api] });

    await store.analyze(root);

    expect(store.clipboardChanged).toBe(false);
    expect(store.results).toEqual({});
    expect(store.pendingCount).toBe(2);
    expect([...store.selectedFor(api.source)].sort()).toEqual(["gone.ts", "index.ts"]);
    expect(store.chunkChoiceFor(api.source, "index.ts")).toBeNull();
    expect(store.fileState(api.source, "index.ts")).toBe("checked");
  });

  it("closing a tab after the clipboard changed leaves the targets of the new clipboard alone", async () => {
    const store = await analyzed();
    await watchClipboard(store, [entry("web", "api")]);

    store.closeProject(web.source);

    expect(store.tabs.map((project) => project.target)).toEqual(["api"]);
    expect({ ...store.targets }).toEqual({ [web.source]: "api" });
  });

  it("changing a target after the clipboard changed leaves the session alone", async () => {
    const store = await analyzed();
    await watchClipboard(store, [entry("web", "api")]);

    store.setTarget(web.source, "");

    expect(store.tabs.map((project) => project.target)).toEqual(["web", "api"]);
  });

  it("goes to the review step after Analyze and back to the clipboard step without losing the session", async () => {
    const store = useTransferStore();
    expect(store.receiveStep).toBe("clipboard");
    expect(store.hasSession).toBe(false);
    store.setReceiveStep("review");
    expect(store.receiveStep).toBe("clipboard");

    await watchClipboard(store, [entry("web"), entry("api"), entry("docs")]);
    backend.analyzeReceive.mockResolvedValue(structuredClone(plan));
    await store.analyze(root);

    expect(store.receiveStep).toBe("review");
    expect(store.hasSession).toBe(true);

    store.setReceiveStep("clipboard");
    expect(store.receiveStep).toBe("clipboard");
    expect(store.tabs).toHaveLength(2);

    store.setReceiveStep("review");
    expect(store.receiveStep).toBe("review");
  });

  it("stays on the clipboard step when Analyze fails", async () => {
    const store = useTransferStore();
    await watchClipboard(store, [entry("web")]);
    backend.analyzeReceive.mockRejectedValue("web: not a project");

    await store.analyze(root);

    expect(store.receiveStep).toBe("clipboard");
    expect(store.hasSession).toBe(false);
    expect(store.receiveError).toBe("web: not a project");
  });

  it("ends the session on Receive more and goes back to the clipboard step", async () => {
    const store = await analyzed();
    await store.apply(root);
    expect(store.pendingCount).toBe(0);
    store.setPreview(web.source, "src/new.ts");

    store.endSession();

    expect(store.hasSession).toBe(false);
    expect(store.plan).toBeNull();
    expect(store.results).toEqual({});
    expect(store.previewFor(web.source)).toBeNull();
    expect(store.receiveStep).toBe("clipboard");
    expect(store.clipboardChanged).toBe(false);
    expect(store.entries).toHaveLength(3);
  });
});

const threeChunks: LineChunk[] = [
  { oldStart: 1, oldCount: 1, newStart: 1, newCount: 1 },
  { oldStart: 4, oldCount: 0, newStart: 4, newCount: 2 },
  { oldStart: 9, oldCount: 1, newStart: 11, newCount: 1 },
];

const indexChunks = { receivedSha256: "r1", localSha256: "l1", chunks: threeChunks };

function hashed(sha256: string): FileSide {
  return { kind: "text", text: "", bom: false, size: 0, sha256, utf8: true };
}

function contents(localSha256: string, receivedSha256: string): ReceiveFileContents {
  return { local: hashed(localSha256), received: hashed(receivedSha256) };
}

describe("transfer store chunk choices", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    resetBackend();
    backend.applyReceive.mockImplementation(async (_root: string, selections: ReceiveSelection[]) =>
      resultFor(selections),
    );
  });

  it("takes every chunk of a replaced file and none of a whitespace-only file until chosen", async () => {
    const store = await analyzed();

    expect(store.isChunkTaken(api.source, "index.ts", 2)).toBe(true);
    expect(store.isChunkTaken(web.source, "src/app.ts", 0)).toBe(false);
    expect(store.fileState(api.source, "index.ts")).toBe("checked");
    expect(store.fileState(web.source, "src/app.ts")).toBe("unchecked");
    expect(store.chunkChoiceFor(api.source, "index.ts")).toBeNull();
  });

  it("marks a file mixed while some chunks are kept, and whole again when all or none are taken", async () => {
    const store = await analyzed();

    store.setChunks(api.source, "index.ts", indexChunks, [1], false);
    expect(store.fileState(api.source, "index.ts")).toBe("indeterminate");
    expect(store.isChunkTaken(api.source, "index.ts", 1)).toBe(false);
    expect([...(store.chunkChoiceFor(api.source, "index.ts")?.taken ?? [])]).toEqual([0, 2]);
    expect([...store.mixedFor(api.source)]).toEqual(["index.ts"]);

    store.setChunks(api.source, "index.ts", indexChunks, [1], true);
    expect(store.fileState(api.source, "index.ts")).toBe("checked");
    expect(store.chunkChoiceFor(api.source, "index.ts")).toBeNull();

    store.setChunks(api.source, "index.ts", indexChunks, [0, 1, 2], false);
    expect(store.fileState(api.source, "index.ts")).toBe("unchecked");
    expect(store.chunkChoiceFor(api.source, "index.ts")).toBeNull();

    store.setChunks(web.source, "src/app.ts", indexChunks, [2], true);
    expect(store.fileState(web.source, "src/app.ts")).toBe("indeterminate");
  });

  it("ignores a choice made on other contents of the file and never carries its chunks over", async () => {
    const store = await analyzed();
    store.setChunks(api.source, "index.ts", indexChunks, [1, 2], false);
    const changed = { receivedSha256: "r2", localSha256: "l1", chunks: [...threeChunks, threeChunks[2] as LineChunk] };

    expect(store.isChunkTaken(api.source, "index.ts", 1, changed)).toBe(true);
    expect(store.fileState(api.source, "index.ts", changed)).toBe("checked");
    expect(store.fileState(api.source, "index.ts")).toBe("indeterminate");
    expect(store.isChunkTaken(api.source, "index.ts", 1)).toBe(false);

    store.setChunks(api.source, "index.ts", changed, [3], false);

    expect(store.chunkChoiceFor(api.source, "index.ts")).toMatchObject({ receivedSha256: "r2", localSha256: "l1" });
    expect([...(store.chunkChoiceFor(api.source, "index.ts")?.taken ?? [])]).toEqual([0, 1, 2]);

    store.setChunks(web.source, "src/app.ts", indexChunks, [0], true);
    store.setChunks(web.source, "src/app.ts", { ...indexChunks, localSha256: "l2" }, [1], true);
    expect([...(store.chunkChoiceFor(web.source, "src/app.ts")?.taken ?? [])]).toEqual([1]);
  });

  it("counts a mixed file once and sends it as a merge of the taken chunks, not as a copied file", async () => {
    const store = await analyzed();
    store.setChunks(api.source, "index.ts", indexChunks, [1], false);

    expect(store.countFor(api.source)).toBe(2);

    await store.apply(root, api.source);

    expect(backend.applyReceive).toHaveBeenCalledWith(root, [
      {
        source: api.source,
        target: "api",
        files: [],
        delete: ["gone.ts"],
        merges: [
          {
            relative: "index.ts",
            receivedSha256: "r1",
            localSha256: "l1",
            total: 3,
            chunks: [threeChunks[0], threeChunks[2]],
          },
        ],
      },
    ]);
  });

  it("applies a project whose only change is a merge", async () => {
    const store = await analyzed();
    store.toggleFile(api.source, "gone.ts");
    store.setChunks(api.source, "index.ts", indexChunks, [0], false);

    await store.apply(root, api.source);

    expect(backend.applyReceive.mock.calls[0]?.[1]?.[0]?.merges).toHaveLength(1);
  });

  it("drops the chunk choice when the file is ticked, unticked or reset by a selection chip", async () => {
    const store = await analyzed();
    store.setChunks(api.source, "index.ts", indexChunks, [1], false);

    store.toggleFile(api.source, "index.ts");
    expect(store.fileState(api.source, "index.ts")).toBe("checked");
    expect(store.chunkChoiceFor(api.source, "index.ts")).toBeNull();

    store.setChunks(api.source, "index.ts", indexChunks, [1], false);
    store.setFiles(api.source, ["index.ts"], false);
    expect(store.fileState(api.source, "index.ts")).toBe("unchecked");
    expect(store.chunkChoiceFor(api.source, "index.ts")).toBeNull();

    store.setChunks(api.source, "index.ts", indexChunks, [1], true);
    store.selectFiles(api.source, "changes", true);
    expect(store.fileState(api.source, "index.ts")).toBe("checked");
    expect(store.chunkChoiceFor(api.source, "index.ts")).toBeNull();
  });

  it("keeps chunk choices on Analyze again when both files are unchanged", async () => {
    const store = await analyzed();
    store.setChunks(api.source, "index.ts", indexChunks, [1], false);
    backend.readReceiveFile.mockResolvedValue(contents("l1", "r1"));

    await store.analyze(root);

    expect(backend.readReceiveFile).toHaveBeenCalledWith(root, api.source, "api", "index.ts");
    expect(store.fileState(api.source, "index.ts")).toBe("indeterminate");
    expect(store.countFor(api.source)).toBe(2);
  });

  it("drops chunk choices on Analyze again when either file changed, back to the default tick", async () => {
    const store = await analyzed();
    store.setChunks(api.source, "index.ts", indexChunks, [1], false);
    store.setChunks(web.source, "src/app.ts", indexChunks, [1], true);
    backend.readReceiveFile.mockImplementation(async (_root: string, source: string) =>
      source === api.source ? contents("l2", "r1") : contents("l1", "r2"),
    );

    await store.analyze(root);

    expect(store.chunkChoiceFor(api.source, "index.ts")).toBeNull();
    expect(store.fileState(api.source, "index.ts")).toBe("checked");
    expect(store.fileState(web.source, "src/app.ts")).toBe("unchecked");
  });

  it("drops a chunk choice on Analyze again when its file cannot be read", async () => {
    const store = await analyzed();
    store.setChunks(api.source, "index.ts", indexChunks, [1], false);
    backend.readReceiveFile.mockRejectedValue(new Error("locked"));

    await store.analyze(root);

    expect(store.chunkChoiceFor(api.source, "index.ts")).toBeNull();
    expect(store.fileState(api.source, "index.ts")).toBe("checked");
    expect(store.receiveError).toBe("");
  });

  it("keeps a chunk choice changed while Analyze again is still reading the file", async () => {
    const store = await analyzed();
    store.setChunks(api.source, "index.ts", indexChunks, [1], false);
    const reading = deferred<ReceiveFileContents>();
    backend.readReceiveFile.mockReturnValueOnce(reading.promise);

    const running = store.analyze(root);
    await vi.waitFor(() => expect(backend.readReceiveFile).toHaveBeenCalledTimes(1));
    store.setChunks(api.source, "index.ts", indexChunks, [0], false);
    reading.resolve(contents("l2", "r1"));
    await running;

    expect([...(store.chunkChoiceFor(api.source, "index.ts")?.taken ?? [])]).toEqual([2]);
    expect(store.fileState(api.source, "index.ts")).toBe("indeterminate");
  });

  it("checks chunk choices against the files again after a failed replace", async () => {
    const store = await analyzed();
    store.setChunks(api.source, "index.ts", indexChunks, [1], false);
    backend.applyReceive.mockRejectedValue(new Error("api: access denied"));
    backend.readReceiveFile.mockResolvedValue(contents("l2", "r1"));

    await store.apply(root, api.source);

    expect(backend.readReceiveFile).toHaveBeenCalledWith(root, api.source, "api", "index.ts");
    expect(store.chunkChoiceFor(api.source, "index.ts")).toBeNull();
    expect(store.fileState(api.source, "index.ts")).toBe("checked");
    expect(store.receiveError).toBe("Error: api: access denied");
  });

  it("drops chunk choices on Analyze again when the file status changed or the file is gone", async () => {
    const store = await analyzed();
    store.setChunks(api.source, "index.ts", indexChunks, [1], false);
    store.setChunks(web.source, "src/app.ts", indexChunks, [1], true);
    backend.readReceiveFile.mockResolvedValue(contents("l1", "r1"));
    const apiAgain = makeProject(api.source, "api", [file("index.ts", "whitespace"), file("gone.ts", "removed")]);
    const webAgain = makeProject(web.source, "web", [file("src/new.ts", "added")]);
    backend.analyzeReceive.mockResolvedValue({ projects: [webAgain, apiAgain, docs] });

    await store.analyze(root);

    expect(store.chunkChoiceFor(api.source, "index.ts")).toBeNull();
    expect(store.fileState(api.source, "index.ts")).toBe("unchecked");
    expect([...store.mixedFor(web.source)]).toEqual([]);
    expect(backend.readReceiveFile).not.toHaveBeenCalled();
  });
});

function echoPlan(requests: ReceiveRequest[]): ReceivePlan {
  return { projects: requests.map((request) => makeProject(request.source, request.target, [file("a.ts", "added")])) };
}

function progressReporter(call = 0): (progress: ExtractProgress) => void {
  return backend.downloadClipboard.mock.calls[call]?.[1] as (progress: ExtractProgress) => void;
}

const halfway: ExtractProgress = { filesDone: 2, filesTotal: 4, bytesDone: 150, bytesTotal: 300 };

describe("transfer store with files from a remote desktop", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    resetBackend();
    backend.analyzeReceive.mockImplementation(async (_root: string, requests: ReceiveRequest[]) => echoPlan(requests));
    backend.applyReceive.mockImplementation(async (_root: string, selections: ReceiveSelection[]) =>
      resultFor(selections),
    );
    backend.cancelClipboardDownload.mockResolvedValue(undefined);
  });

  it("downloads the clipboard first and analyzes the downloaded folders, matched by id", async () => {
    const store = useTransferStore();
    await poll(store, remote(7, ["web", "api"], 2));
    backend.downloadClipboard.mockResolvedValue(downloaded(7, ["api", "web"]));

    await store.analyze(root);

    expect(backend.downloadClipboard).toHaveBeenCalledWith(7, expect.any(Function));
    expect(backend.analyzeReceive).toHaveBeenCalledWith(
      root,
      [
        { source: receivedPath(7, "web"), target: "web" },
        { source: receivedPath(7, "api"), target: "api" },
      ],
      [],
      [],
    );
    expect(backend.downloadClipboard.mock.invocationCallOrder[0]).toBeLessThan(
      backend.analyzeReceive.mock.invocationCallOrder[0] ?? 0,
    );
    expect(store.tabs.map((project) => project.source)).toEqual([receivedPath(7, "web"), receivedPath(7, "api")]);
    expect(store.clipboard).toEqual({ source: "virtual", sequence: 7, rejected: 2, problem: null });
    expect(store.download?.directory).toBe("C:\\recv\\7");
    expect(store.folderFor(remoteId(7, "web"))).toBe(receivedPath(7, "web"));
  });

  it("fails without analyzing and forgets the download when a requested folder is missing from it", async () => {
    const store = useTransferStore();
    await poll(store, remote(7, ["web", "api"]));
    backend.downloadClipboard.mockResolvedValue(downloaded(7, ["api"]));

    await store.analyze(root);

    expect(backend.analyzeReceive).not.toHaveBeenCalled();
    expect(store.receiveError).toBe("web is missing from the download, copy the folders again on the remote desktop");
    expect(store.plan).toBeNull();
    expect(store.download).toBeNull();
    expect(store.folderFor(remoteId(7, "api"))).toBeNull();

    backend.downloadClipboard.mockResolvedValue(downloaded(7, ["web", "api"]));
    await store.analyze(root);

    expect(backend.downloadClipboard).toHaveBeenCalledTimes(2);
    expect(store.tabs).toHaveLength(2);
  });

  it("fails without downloading when a remote clipboard has no sequence", async () => {
    const store = useTransferStore();
    await poll(store, { ...remote(7, ["web"]), sequence: null });

    await store.analyze(root);

    expect(backend.downloadClipboard).not.toHaveBeenCalled();
    expect(backend.analyzeReceive).not.toHaveBeenCalled();
    expect(store.receiveError).toContain("web is missing from the download");
  });

  it("asks the backend on every analyze, which returns its kept download, without showing the progress", async () => {
    const store = useTransferStore();
    await poll(store, remote(7, ["web"]));
    backend.downloadClipboard.mockResolvedValue(downloaded(7, ["web"]));
    await store.analyze(root);
    const reused = deferred<ClipboardDownload>();
    backend.downloadClipboard.mockReturnValueOnce(reused.promise);

    const again = store.analyze(root);
    await vi.waitFor(() => expect(backend.downloadClipboard).toHaveBeenCalledTimes(2));
    expect(store.downloading).toBe(true);
    expect(store.downloadShown).toBe(false);
    reused.resolve(downloaded(7, ["web"]));
    await again;
    store.toggleReceivePattern("*.log");
    await store.analyze(root);

    expect(backend.downloadClipboard.mock.calls.map(([sequence]) => sequence)).toEqual([7, 7, 7]);
    expect(backend.analyzeReceive).toHaveBeenCalledTimes(3);
    expect(backend.analyzeReceive.mock.calls[2]?.[1]).toEqual([{ source: receivedPath(7, "web"), target: "web" }]);
    expect(store.downloadShown).toBe(false);
  });

  it("shows the progress after a short delay when the download has not reported yet", async () => {
    const store = useTransferStore();
    await poll(store, remote(7, ["web"]));
    const pending = deferred<ClipboardDownload>();
    backend.downloadClipboard.mockReturnValueOnce(pending.promise);
    vi.useFakeTimers();
    try {
      const running = store.analyze(root);
      await vi.waitFor(() => expect(backend.downloadClipboard).toHaveBeenCalled());
      expect(store.downloadShown).toBe(false);

      vi.advanceTimersByTime(1000);
      expect(store.downloadShown).toBe(true);
      expect(store.downloadProgress).toBeNull();

      pending.resolve(downloaded(7, ["web"]));
      await running;
      expect(store.downloadShown).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });

  it("resets the targets and keeps the session when the clipboard sequence changes, and downloads again on Analyze", async () => {
    const store = useTransferStore();
    await poll(store, remote(7, ["web", "api"]));
    backend.downloadClipboard.mockResolvedValue(downloaded(7, ["web", "api"]));
    store.setTarget(remoteId(7, "api"), "docs");
    await store.analyze(root);
    expect(store.tabs).toHaveLength(2);

    await poll(store, remote(8, ["web", "api"]));

    expect(store.tabs.map((project) => project.source)).toEqual([receivedPath(7, "web"), receivedPath(7, "api")]);
    expect(store.clipboardChanged).toBe(true);
    expect(store.download).toBeNull();
    expect(store.folderFor(remoteId(8, "web"))).toBeNull();
    expect({ ...store.targets }).toEqual({ [remoteId(8, "web")]: "web", [remoteId(8, "api")]: "api" });

    backend.downloadClipboard.mockResolvedValue(downloaded(8, ["web", "api"]));
    await store.analyze(root);

    expect(backend.downloadClipboard).toHaveBeenCalledTimes(2);
    expect(backend.downloadClipboard).toHaveBeenLastCalledWith(8, expect.any(Function));
    expect(store.tabs.map((project) => project.source)).toEqual([receivedPath(8, "web"), receivedPath(8, "api")]);
  });

  it("treats a new sequence with the same folder ids as another clipboard", async () => {
    const store = useTransferStore();
    await poll(store, remote(7, ["web"]));
    backend.downloadClipboard.mockResolvedValue(downloaded(7, ["web"]));
    await store.analyze(root);
    expect(store.download).not.toBeNull();

    await poll(store, { ...remote(7, ["web"]), sequence: 8 });

    expect(store.download).toBeNull();
    expect(store.clipboardChanged).toBe(true);
    expect(store.tabs).toHaveLength(1);

    await poll(store, remote(7, ["web"]));

    expect(store.download?.directory).toBe("C:\\recv\\7");
    expect(store.clipboardChanged).toBe(false);
  });

  it("shows the download progress while it runs, with the other actions busy", async () => {
    const store = useTransferStore();
    await poll(store, remote(7, ["web"]));
    const pending = deferred<ClipboardDownload>();
    backend.downloadClipboard.mockReturnValueOnce(pending.promise);

    const running = store.analyze(root);
    await vi.waitFor(() => expect(backend.downloadClipboard).toHaveBeenCalled());

    expect(store.downloading).toBe(true);
    expect(store.receiveBusy).toBe(true);
    expect(store.downloadProgress).toBeNull();
    progressReporter()(halfway);
    expect(store.downloadProgress).toEqual(halfway);
    expect(store.downloadShown).toBe(true);

    pending.resolve(downloaded(7, ["web"]));
    await running;
    progressReporter()(halfway);

    expect(store.downloading).toBe(false);
    expect(store.downloadShown).toBe(false);
    expect(store.downloadProgress).toBeNull();
    expect(store.tabs).toHaveLength(1);
  });

  it("cancels the download, shows the backend error, leaves no plan and downloads again next time", async () => {
    const store = useTransferStore();
    await poll(store, remote(7, ["web"]));
    const pending = deferred<ClipboardDownload>();
    backend.downloadClipboard.mockReturnValueOnce(pending.promise);
    backend.cancelClipboardDownload.mockImplementation(async () => pending.reject("The download was cancelled"));

    const running = store.analyze(root);
    await vi.waitFor(() => expect(backend.downloadClipboard).toHaveBeenCalled());
    await store.cancelDownload();
    expect(backend.cancelClipboardDownload).toHaveBeenCalledTimes(1);
    await running;

    expect(store.receiveError).toBe("The download was cancelled");
    expect(store.plan).toBeNull();
    expect(store.download).toBeNull();
    expect(store.downloading).toBe(false);
    expect(backend.analyzeReceive).not.toHaveBeenCalled();

    backend.downloadClipboard.mockResolvedValueOnce(downloaded(7, ["web"]));
    await store.analyze(root);

    expect(backend.downloadClipboard).toHaveBeenCalledTimes(2);
    expect(store.tabs).toHaveLength(1);
    expect(store.receiveError).toBe("");
  });

  it("shows the error of a cancel that fails", async () => {
    const store = useTransferStore();
    await poll(store, remote(7, ["web"]));
    backend.cancelClipboardDownload.mockRejectedValue("The receive state is not available");

    await store.cancelDownload();

    expect(store.receiveError).toBe("The receive state is not available");
  });

  it("analyzes a download that finishes after the clipboard changed, as the session of the analyzed clipboard", async () => {
    const store = useTransferStore();
    await poll(store, remote(7, ["web"]));
    const pending = deferred<ClipboardDownload>();
    backend.downloadClipboard.mockReturnValueOnce(pending.promise);

    const running = store.analyze(root);
    await poll(store, remote(8, ["web"]));
    progressReporter()(halfway);
    expect(store.downloadProgress).toEqual(halfway);
    pending.resolve(downloaded(7, ["web"]));
    await running;

    expect(backend.analyzeReceive).toHaveBeenCalledWith(
      root,
      [{ source: receivedPath(7, "web"), target: "web" }],
      [],
      [],
    );
    expect(store.tabs.map((project) => project.source)).toEqual([receivedPath(7, "web")]);
    expect(store.clipboardChanged).toBe(true);
    expect(store.download).toBeNull();
    expect(store.downloading).toBe(false);
    expect(store.entries.map((entry) => entry.path)).toEqual([remoteId(8, "web")]);
  });

  it("keeps showing the newer download while an older one ends", async () => {
    const store = useTransferStore();
    await poll(store, remote(7, ["web"]));
    const older = deferred<ClipboardDownload>();
    const newer = deferred<ClipboardDownload>();
    backend.downloadClipboard.mockReturnValueOnce(older.promise).mockReturnValueOnce(newer.promise);

    const stale = store.analyze(root);
    await vi.waitFor(() => expect(backend.downloadClipboard).toHaveBeenCalledTimes(1));
    progressReporter(0)(halfway);
    await poll(store, remote(8, ["web"]));
    const running = store.analyze(root);
    await vi.waitFor(() => expect(backend.downloadClipboard).toHaveBeenCalledTimes(2));

    expect(store.downloadProgress).toBeNull();
    older.resolve(downloaded(7, ["web"]));
    await stale;
    expect(store.downloading).toBe(true);
    progressReporter(1)(halfway);
    expect(store.downloadProgress).toEqual(halfway);

    newer.resolve(downloaded(8, ["web"]));
    await running;
    expect(store.downloading).toBe(false);
    expect(store.tabs.map((project) => project.source)).toEqual([receivedPath(8, "web")]);
  });

  it("shows the error of a download that fails because the clipboard changed and keeps the earlier session", async () => {
    const store = useTransferStore();
    await poll(store, remote(6, ["web"]));
    backend.downloadClipboard.mockResolvedValueOnce(downloaded(6, ["web"]));
    await store.analyze(root);
    await poll(store, remote(7, ["web"]));
    const pending = deferred<ClipboardDownload>();
    backend.downloadClipboard.mockReturnValueOnce(pending.promise);

    const running = store.analyze(root);
    await poll(store, remote(8, ["web"]));
    pending.reject("The clipboard changed, analyze again");
    await running;

    expect(store.receiveError).toBe("The clipboard changed, analyze again");
    expect(store.tabs.map((project) => project.source)).toEqual([receivedPath(6, "web")]);
    expect(store.clipboardChanged).toBe(true);
  });

  it("keeps the targets and the running download when a poll returns the same clipboard", async () => {
    const store = useTransferStore();
    await poll(store, remote(7, ["web", "api"]));
    store.setTarget(remoteId(7, "api"), "docs");
    const pending = deferred<ClipboardDownload>();
    backend.downloadClipboard.mockReturnValueOnce(pending.promise);

    const running = store.analyze(root);
    await poll(store, remote(7, ["web", "api"]));
    progressReporter()(halfway);
    expect(store.downloadProgress).toEqual(halfway);
    pending.resolve(downloaded(7, ["web", "api"]));
    await running;

    expect(store.targets[remoteId(7, "api")]).toBe("docs");
    expect(backend.analyzeReceive).toHaveBeenCalledWith(
      root,
      [
        { source: receivedPath(7, "web"), target: "web" },
        { source: receivedPath(7, "api"), target: "docs" },
      ],
      [],
      [],
    );
    expect(store.tabs).toHaveLength(2);
    expect(store.download?.sequence).toBe(7);
  });

  it("keeps the folders, targets, download and plan while the clipboard is busy", async () => {
    const store = useTransferStore();
    await poll(store, remote(7, ["web"]));
    backend.downloadClipboard.mockResolvedValue(downloaded(7, ["web"]));
    store.setTarget(remoteId(7, "web"), "docs");
    await store.analyze(root);

    await poll(store, busy);

    expect(store.clipboardBusy).toBe(true);
    expect(store.clipboard.source).toBe("virtual");
    expect(store.entries).toHaveLength(1);
    expect(store.targets[remoteId(7, "web")]).toBe("docs");
    expect(store.tabs).toHaveLength(1);

    await poll(store, remote(7, ["web"]));
    await store.analyze(root);

    expect(store.clipboardBusy).toBe(false);
    expect(store.tabs).toHaveLength(1);
    expect(backend.downloadClipboard.mock.calls.map(([sequence]) => sequence)).toEqual([7, 7]);
  });

  it("clears the folders, keeps the session and shows the problem when the clipboard cannot be read", async () => {
    const store = useTransferStore();
    await poll(store, remote(7, ["web"]));
    backend.downloadClipboard.mockResolvedValue(downloaded(7, ["web"]));
    await store.analyze(root);

    await poll(store, { ...busy, source: "unreadable", problem: "The clipboard file list is malformed" });

    expect(store.clipboard).toEqual({
      source: "unreadable",
      sequence: null,
      rejected: 0,
      problem: "The clipboard file list is malformed",
    });
    expect(store.entries).toEqual([]);
    expect(store.tabs).toHaveLength(1);
    expect(store.clipboardChanged).toBe(true);
    expect(store.download).toBeNull();
  });

  it("never downloads a file list", async () => {
    const store = await analyzed();
    await store.analyze(root);

    expect(backend.downloadClipboard).not.toHaveBeenCalled();
    expect(store.clipboard.source).toBe("paths");
    expect(store.folderFor(web.source)).toBe(web.source);
  });

  it("closing the tab of a downloaded folder stops receiving that clipboard folder", async () => {
    const store = useTransferStore();
    await poll(store, remote(7, ["web", "api"]));
    backend.downloadClipboard.mockResolvedValue(downloaded(7, ["web", "api"]));
    await store.analyze(root);

    store.closeProject(receivedPath(7, "web"));

    expect(store.targets[remoteId(7, "web")]).toBe("");
    expect(store.tabs.map((project) => project.source)).toEqual([receivedPath(7, "api")]);
    expect(store.requests).toEqual([{ source: remoteId(7, "api"), target: "api" }]);

    store.setTarget(remoteId(7, "api"), "docs");

    expect(store.tabs).toEqual([]);
  });

  it("analyzes the downloaded folders of the session again after a failed replace, without downloading", async () => {
    const store = useTransferStore();
    await poll(store, remote(7, ["web"]));
    backend.downloadClipboard.mockResolvedValue(downloaded(7, ["web"]));
    await store.analyze(root);
    backend.analyzeReceive.mockClear();
    backend.applyReceive.mockRejectedValue(new Error("web: access denied"));

    await store.apply(root);

    expect(backend.analyzeReceive).toHaveBeenCalledWith(
      root,
      [{ source: receivedPath(7, "web"), target: "web" }],
      [],
      [],
    );
    expect(backend.downloadClipboard).toHaveBeenCalledTimes(1);
    expect(store.receiveError).toBe("Error: web: access denied");
  });

  it("analyzes the session folders, not the new clipboard, when a replace fails after the clipboard changed", async () => {
    const store = useTransferStore();
    await poll(store, remote(7, ["web", "api"]));
    backend.downloadClipboard.mockResolvedValue(downloaded(7, ["web", "api"]));
    await store.analyze(root);
    const replacing = deferred<ReceiveResult>();
    backend.applyReceive.mockReturnValueOnce(replacing.promise);
    backend.analyzeReceive.mockClear();

    const running = store.apply(root);
    await poll(store, remote(8, ["other"]));
    replacing.reject(new Error("web: access denied"));
    await running;

    expect(backend.downloadClipboard).toHaveBeenCalledTimes(1);
    expect(backend.analyzeReceive).toHaveBeenCalledWith(
      root,
      [
        { source: receivedPath(7, "web"), target: "web" },
        { source: receivedPath(7, "api"), target: "api" },
      ],
      [],
      [],
    );
    expect(store.downloading).toBe(false);
    expect(store.tabs).toHaveLength(2);
    expect(store.clipboardChanged).toBe(true);
    expect(store.receiveError).toBe("Error: web: access denied");
  });

  it("closing the tab of a downloaded folder after the clipboard changed leaves the new targets alone", async () => {
    const store = useTransferStore();
    await poll(store, remote(7, ["web", "api"]));
    backend.downloadClipboard.mockResolvedValue(downloaded(7, ["web", "api"]));
    await store.analyze(root);
    await poll(store, remote(8, ["web"]));

    store.closeProject(receivedPath(7, "web"));

    expect(store.tabs.map((project) => project.source)).toEqual([receivedPath(7, "api")]);
    expect({ ...store.targets }).toEqual({ [remoteId(8, "web")]: "web" });
  });
});
