import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

import type {
  ClipboardContents,
  ClipboardDownload,
  ClipboardEntry,
  ExtractProgress,
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
    await store.toggleReceivePattern(root, "*.log");
    expect(backend.analyzeReceive).not.toHaveBeenCalled();

    await store.analyze(root);

    expect(backend.analyzeReceive).toHaveBeenCalledWith(
      root,
      [{ source: "C:\\clip\\web", target: "web" }],
      [],
      ["*.log"],
    );
  });

  it("analyzes again when a Receive pattern is toggled while a plan is shown", async () => {
    const store = await analyzed();
    backend.analyzeReceive.mockClear();
    const apiWithoutRemoval = makeProject(api.source, "api", [file("index.ts", "replaced")]);
    backend.analyzeReceive.mockResolvedValue({ projects: [web, apiWithoutRemoval, docs] });

    await store.toggleReceivePattern(root, ".env.local");

    expect(backend.analyzeReceive).toHaveBeenCalledTimes(1);
    expect(backend.analyzeReceive.mock.calls[0]?.[3]).toEqual([".env.local"]);
    expect(relatives(store.tabs.find((project) => project.source === api.source))).toEqual(["index.ts"]);

    await store.toggleReceivePattern(root, ".env.local");
    expect(backend.analyzeReceive).toHaveBeenCalledTimes(2);
    expect(backend.analyzeReceive.mock.calls[1]?.[3]).toEqual([]);
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

  it("shows the plan of the latest toggle when the earlier analyze answers last", async () => {
    const store = await analyzed();
    const first = deferred<ReceivePlan>();
    const second = deferred<ReceivePlan>();
    backend.analyzeReceive.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);

    const firstToggle = store.toggleReceivePattern(root, "*.log");
    const secondToggle = store.toggleReceivePattern(root, "*.map");
    second.resolve({ projects: [makeProject(web.source, "web", [file("src/second.ts", "added")])] });
    await secondToggle;
    first.resolve({ projects: [makeProject(web.source, "web", [file("src/first.ts", "added")])] });
    await firstToggle;

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

    const firstToggle = store.toggleReceivePattern(root, "*.log");
    await store.toggleReceivePattern(root, "*.map");
    first.reject(new Error("stale"));
    await firstToggle;

    expect(store.receiveError).toBe("");
    expect(store.tabs.map(relatives)).toEqual([["src/second.ts"]]);
  });

  it("keeps applied results and manual ticks when a pattern toggle analyzes again", async () => {
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

    await store.toggleReceivePattern(root, "*.log");

    expect(store.results[web.source]?.result.files).toEqual(["src/new.ts"]);
    expect(store.tabs.map((project) => project.target)).toEqual(["web", "api"]);
    expect(relatives(store.tabs[1])).toEqual(["index.ts", "gone.ts", "extra.ts"]);
    expect([...store.selectedFor(api.source)].sort()).toEqual(["extra.ts", "index.ts"]);
    expect(store.activeSource).toBe(api.source);
  });

  it("analyzes the shown plan again after a failed replace and keeps the error visible", async () => {
    const store = await analyzed();
    await store.toggleReceivePattern(root, "*.log");
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

  it("drops an analyze answer that arrives after the clipboard changed", async () => {
    const store = await analyzed();
    const pending = deferred<ReceivePlan>();
    backend.analyzeReceive.mockReturnValueOnce(pending.promise);

    const running = store.analyze(root);
    await watchClipboard(store, [entry("other")]);
    pending.resolve(structuredClone(plan));
    await running;

    expect(store.plan).toBeNull();
    expect(store.tabs).toEqual([]);
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
    await store.toggleReceivePattern(root, "*.log");

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

  it("forgets the download, the plan and the targets when the clipboard sequence changes", async () => {
    const store = useTransferStore();
    await poll(store, remote(7, ["web", "api"]));
    backend.downloadClipboard.mockResolvedValue(downloaded(7, ["web", "api"]));
    store.setTarget(remoteId(7, "api"), "docs");
    await store.analyze(root);
    expect(store.tabs).toHaveLength(2);

    await poll(store, remote(8, ["web", "api"]));

    expect(store.plan).toBeNull();
    expect(store.download).toBeNull();
    expect({ ...store.targets }).toEqual({ [remoteId(8, "web")]: "web", [remoteId(8, "api")]: "api" });

    backend.downloadClipboard.mockResolvedValue(downloaded(8, ["web", "api"]));
    await store.analyze(root);

    expect(backend.downloadClipboard).toHaveBeenCalledTimes(2);
    expect(backend.downloadClipboard).toHaveBeenLastCalledWith(8, expect.any(Function));
    expect(store.tabs.map((project) => project.source)).toEqual([receivedPath(8, "web"), receivedPath(8, "api")]);
  });

  it("forgets the download when only the sequence changes, not the folder ids", async () => {
    const store = useTransferStore();
    await poll(store, remote(7, ["web"]));
    backend.downloadClipboard.mockResolvedValue(downloaded(7, ["web"]));
    await store.analyze(root);
    expect(store.download).not.toBeNull();

    await poll(store, { ...remote(7, ["web"]), sequence: 8 });

    expect(store.download).toBeNull();
    expect(store.plan).toBeNull();
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

  it("drops a download that finishes after the clipboard changed", async () => {
    const store = useTransferStore();
    await poll(store, remote(7, ["web"]));
    const pending = deferred<ClipboardDownload>();
    backend.downloadClipboard.mockReturnValueOnce(pending.promise);

    const running = store.analyze(root);
    await poll(store, remote(8, ["web"]));
    progressReporter()(halfway);
    expect(store.downloadProgress).toBeNull();
    pending.resolve(downloaded(7, ["web"]));
    await running;

    expect(backend.analyzeReceive).not.toHaveBeenCalled();
    expect(store.plan).toBeNull();
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

  it("does not show the error of a download that fails after the clipboard changed", async () => {
    const store = useTransferStore();
    await poll(store, remote(7, ["web"]));
    const pending = deferred<ClipboardDownload>();
    backend.downloadClipboard.mockReturnValueOnce(pending.promise);

    const running = store.analyze(root);
    await poll(store, remote(8, ["web"]));
    pending.reject("The clipboard changed, analyze again");
    await running;

    expect(store.receiveError).toBe("");
    expect(store.plan).toBeNull();
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

  it("clears the folders and shows the problem when the clipboard cannot be read", async () => {
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
    expect(store.plan).toBeNull();
    expect(store.download).toBeNull();
  });

  it("never downloads a file list", async () => {
    const store = await analyzed();
    await store.toggleReceivePattern(root, "*.log");

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

  it("analyzes the downloaded folders again after a failed replace, asking the backend for its kept download", async () => {
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
    expect(backend.downloadClipboard.mock.calls.map(([sequence]) => sequence)).toEqual([7, 7]);
    expect(store.receiveError).toBe("Error: web: access denied");
  });

  it("does not download a new clipboard when a replace fails after the clipboard changed", async () => {
    const store = useTransferStore();
    await poll(store, remote(7, ["web"]));
    backend.downloadClipboard.mockResolvedValue(downloaded(7, ["web"]));
    await store.analyze(root);
    const replacing = deferred<ReceiveResult>();
    backend.applyReceive.mockReturnValueOnce(replacing.promise);

    const running = store.apply(root);
    await poll(store, remote(8, ["web"]));
    replacing.reject(new Error("web: access denied"));
    await running;

    expect(backend.downloadClipboard).toHaveBeenCalledTimes(1);
    expect(store.downloading).toBe(false);
    expect(store.plan).toBeNull();
    expect(store.receiveError).toBe("Error: web: access denied");
  });
});
