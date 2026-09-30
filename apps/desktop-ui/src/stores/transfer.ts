import { computed, reactive, ref } from "vue";
import { useIntervalFn } from "@vueuse/core";
import { defineStore } from "pinia";

import * as api from "@/api/commands";
import type {
  ClipboardContents,
  ClipboardDownload,
  ClipboardEntry,
  CopyResult,
  DownloadedFolder,
  ExtractProgress,
  FileMerge,
  FileSide,
  LineChunk,
  ReceiveFileContents,
  ReceivePlan,
  ReceiveProjectPlan,
  ReceiveProjectResult,
  ReceiveRequest,
  ReceiveSelection,
  TransferPreview,
} from "@/api/types";
import { describeError, useAsyncAction } from "@/composables/useAsyncAction";
import { i18n } from "@/i18n";
import { clipboardIdFor, downloadedFolder, toDownloadedRequests } from "@/lib/clipboard-download";
import {
  carrySelection,
  defaultExpanded,
  selectedRemovals,
  selectionFor,
  splitByChanges,
  toSelection,
  visibleFiles,
  type CheckState,
  type RemovalGroup,
  type SelectionMode,
} from "@/lib/receive-view";

export type TransferTab = "copy" | "receive";

export type ReceiveStep = "clipboard" | "review";

export interface ReceivedProject {
  result: ReceiveProjectResult;
  receivedAt: string;
  logFile: string | null;
}

interface AnalyzeInput {
  requests: ReceiveRequest[];
  patterns: string[];
  disabled: string[];
}

interface ReceiveSession {
  signature: string;
  download: ClipboardDownload | null;
  patterns: string[];
  disabled: string[];
}

interface FoundPlan {
  plan: ReceivePlan;
  download: ClipboardDownload | null;
}

export type ClipboardStatus = Omit<ClipboardContents, "entries">;

/** The chunks of one file's exact line diff, and the hashes of the two sides they came from. */
export interface ChunkFile {
  receivedSha256: string;
  localSha256: string;
  chunks: LineChunk[];
}

/** Which chunks of a file to take; only stored while some but not all are taken. */
export interface ChunkChoice extends ChunkFile {
  taken: ReadonlySet<number>;
}

type ChunkChoices = Record<string, ChunkChoice>;

const pollMs = 2000;

const showDownloadAfterMs = 300;

const fileCacheSize = 20;

const noSelection: ReadonlySet<string> = new Set();

const noChoices: Readonly<ChunkChoices> = {};

function textSha(side: FileSide | null): string | null {
  return side?.kind === "text" ? side.sha256 : null;
}

function toMerge(relative: string, choice: ChunkChoice): FileMerge {
  return {
    relative,
    receivedSha256: choice.receivedSha256,
    localSha256: choice.localSha256,
    total: choice.chunks.length,
    chunks: choice.chunks.filter((_chunk, index) => choice.taken.has(index)),
  };
}

function parsePatterns(text: string): string[] {
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
}

function toggled(list: readonly string[], value: string): string[] {
  return list.includes(value) ? list.filter((entry) => entry !== value) : [...list, value];
}

function clear(record: Record<string, unknown>): void {
  for (const key of Object.keys(record)) delete record[key];
}

export const useTransferStore = defineStore("transfer", () => {
  const tab = ref<TransferTab>("copy");

  const copyProjects = ref<string[]>([]);
  const copyPatterns = ref("");
  const copyDisabled = ref<string[]>([]);
  const preview = ref<TransferPreview | null>(null);
  const copyResult = ref<CopyResult | null>(null);
  const copyAction = useAsyncAction();

  const entries = ref<ClipboardEntry[]>([]);
  const targets = reactive<Record<string, string>>({});
  const receivePatterns = ref("");
  const receiveDisabled = ref<string[]>([]);
  const plan = ref<ReceivePlan | null>(null);
  const activeSource = ref<string | null>(null);
  const selected = reactive<Record<string, Set<string>>>({});
  const chunkChoices = reactive<Record<string, ChunkChoices>>({});
  const expanded = reactive<Record<string, Set<string>>>({});
  const results = reactive<Record<string, ReceivedProject>>({});
  const previews = reactive<Record<string, string>>({});
  const fileCache = new Map<string, Promise<ReceiveFileContents>>();
  const receiveAction = useAsyncAction();
  const watching = ref(false);
  const clipboard = ref<ClipboardStatus>({ source: "empty", sequence: null, rejected: 0, problem: null });
  const clipboardBusy = ref(false);
  const downloading = ref(false);
  const downloadShown = ref(false);
  const downloadProgress = ref<ExtractProgress | null>(null);
  const receiveStep = ref<ReceiveStep>("clipboard");
  const session = ref<ReceiveSession | null>(null);
  const liveSignature = ref("");

  let watchedRoot = "";
  let analyzeRequest = 0;
  let downloadCall = 0;

  const hasSession = computed(() => session.value !== null);
  const clipboardChanged = computed(() => session.value !== null && session.value.signature !== liveSignature.value);
  const download = computed(() => (clipboardChanged.value ? null : (session.value?.download ?? null)));
  const copySelectionEmpty = computed(() => copyProjects.value.length === 0);
  const requests = computed(() =>
    entries.value
      .filter((entry) => targets[entry.path])
      .map((entry) => ({ source: entry.path, target: targets[entry.path] ?? "" })),
  );
  const split = computed(() => splitByChanges(plan.value?.projects ?? []));
  const tabs = computed(() => split.value.changed);
  const unchangedProjects = computed(() => split.value.unchanged);
  const pendingTabs = computed(() => tabs.value.filter((project) => !results[project.source]));
  const pendingCount = computed(() => pendingTabs.value.length);
  const selectedCount = computed(() => pendingTabs.value.reduce((sum, project) => sum + countFor(project.source), 0));

  function selectedFor(source: string): ReadonlySet<string> {
    return selected[source] ?? noSelection;
  }

  function project(source: string): ReceiveProjectPlan | undefined {
    return plan.value?.projects.find((entry) => entry.source === source);
  }

  function choicesFor(source: string): Readonly<ChunkChoices> {
    return chunkChoices[source] ?? noChoices;
  }

  function selectionOf(entry: ReceiveProjectPlan): ReceiveSelection {
    const selection = toSelection(entry, selectedFor(entry.source));
    const choices = choicesFor(entry.source);
    const merges = entry.files.flatMap((file) => {
      const choice = choices[file.relative];
      return choice ? [toMerge(file.relative, choice)] : [];
    });
    return merges.length ? { ...selection, merges } : selection;
  }

  function countFor(source: string): number {
    const found = project(source);
    if (!found) return 0;
    const selection = selectionOf(found);
    return selection.files.length + selection.delete.length + (selection.merges?.length ?? 0);
  }

  function projectsToApply(source?: string): ReceiveProjectPlan[] {
    return source === undefined ? pendingTabs.value : pendingTabs.value.filter((entry) => entry.source === source);
  }

  function removalsFor(source?: string): RemovalGroup[] {
    return selectedRemovals(projectsToApply(source), selectedFor);
  }

  function setCopyProjects(names: string[]): void {
    copyProjects.value = [...new Set(names)];
    preview.value = null;
  }

  function toggleCopyProject(name: string): void {
    const next = new Set(copyProjects.value);
    if (next.has(name)) next.delete(name);
    else next.add(name);
    setCopyProjects([...next]);
  }

  function toggleCopyPattern(pattern: string): void {
    copyDisabled.value = toggled(copyDisabled.value, pattern);
    preview.value = null;
  }

  async function previewCopy(root: string): Promise<void> {
    await copyAction.run(async () => {
      preview.value = await api.previewTransfer(
        root,
        copyProjects.value,
        parsePatterns(copyPatterns.value),
        copyDisabled.value,
      );
    });
  }

  async function copy(root: string): Promise<void> {
    await copyAction.run(async () => {
      copyResult.value = await api.copyProjectsToClipboard(
        root,
        copyProjects.value,
        parsePatterns(copyPatterns.value),
        copyDisabled.value,
      );
      preview.value = null;
    });
  }

  function resetPlan(next: ReceivePlan | null): void {
    plan.value = next;
    clear(selected);
    clear(chunkChoices);
    clear(expanded);
    clear(results);
    clear(previews);
    fileCache.clear();
    for (const entry of next?.projects ?? []) {
      selected[entry.source] = new Set(selectionFor(entry.files, "changes"));
      expanded[entry.source] = defaultExpanded(entry.files);
    }
    activeSource.value = tabs.value[0]?.source ?? null;
  }

  function applyContents(contents: ClipboardContents): void {
    clipboardBusy.value = contents.source === "busy";
    if (clipboardBusy.value) return;

    const { entries: next, ...status } = contents;
    clipboard.value = status;
    const signature = [status.sequence ?? "", ...next.map((entry) => entry.path)].join("|");
    if (signature === liveSignature.value) return;

    liveSignature.value = signature;
    entries.value = next;
    clear(targets);
    for (const entry of next) targets[entry.path] = entry.suggestedProject ?? "";
  }

  async function poll(root: string): Promise<void> {
    try {
      applyContents(await api.inspectClipboard(root));
    } catch (cause) {
      receiveAction.error.value = describeError(cause);
    }
  }

  const poller = useIntervalFn(() => void poll(watchedRoot), pollMs, { immediate: false });

  function startWatching(root: string): void {
    if (poller.isActive.value) return;
    watchedRoot = root;
    watching.value = true;
    void poll(root);
    poller.resume();
  }

  function stopWatching(): void {
    poller.pause();
    watching.value = false;
  }

  function dropProject(source: string): void {
    if (plan.value) plan.value = { projects: plan.value.projects.filter((entry) => entry.source !== source) };
    delete selected[source];
    delete chunkChoices[source];
    delete expanded[source];
    delete results[source];
    delete previews[source];
    if (activeSource.value === source || !tabs.value.some((entry) => entry.source === activeSource.value)) {
      activeSource.value = tabs.value[0]?.source ?? null;
    }
  }

  function folderFor(path: string): string | null {
    if (clipboard.value.source !== "virtual") return path;
    return download.value ? downloadedFolder(path, download.value.folders) : null;
  }

  function setTarget(path: string, target: string): void {
    targets[path] = target;
    if (hasSession.value && !clipboardChanged.value) dropProject(folderFor(path) ?? path);
  }

  function closeProject(source: string): void {
    const id = clipboardIdFor(source, session.value?.download?.folders ?? []);
    dropProject(source);
    if (!clipboardChanged.value && id in targets) targets[id] = "";
  }

  function setActive(source: string): void {
    activeSource.value = source;
  }

  function setReceiveStep(step: ReceiveStep): void {
    if (step === "review" && !hasSession.value) return;
    receiveStep.value = step;
  }

  function carryChoices(
    previous: ReceiveProjectPlan | undefined,
    next: ReceiveProjectPlan,
    choices: ChunkChoices,
  ): void {
    const before = new Map(previous?.files.map((file) => [file.relative, file.status]));
    const kept: ChunkChoices = {};
    for (const file of next.files) {
      const choice = choices[file.relative];
      if (choice && before.get(file.relative) === file.status) kept[file.relative] = choice;
    }
    chunkChoices[next.source] = kept;
  }

  function keepPlan(next: ReceivePlan): void {
    const before = new Map((plan.value?.projects ?? []).map((entry) => [entry.source, entry]));
    const ticks = { ...selected };
    const choices = { ...chunkChoices };
    const received = { ...results };
    const projects = next.projects.map((entry) =>
      received[entry.source] ? (before.get(entry.source) ?? entry) : entry,
    );

    resetPlan({ projects });
    for (const entry of projects) {
      const result = received[entry.source];
      if (result) results[entry.source] = result;
      selected[entry.source] = new Set(
        carrySelection(before.get(entry.source), entry, ticks[entry.source] ?? noSelection),
      );
      carryChoices(before.get(entry.source), entry, choices[entry.source] ?? {});
    }
  }

  function dropChoice(source: string, relative: string): void {
    const choices = chunkChoices[source];
    if (!choices?.[relative]) return;
    chunkChoices[source] = Object.fromEntries(Object.entries(choices).filter(([path]) => path !== relative));
  }

  async function stillSame(root: string, source: string, relative: string, choice: ChunkChoice): Promise<boolean> {
    try {
      const read = await readFile(root, source, relative);
      return textSha(read.local) === choice.localSha256 && textSha(read.received) === choice.receivedSha256;
    } catch {
      return false;
    }
  }

  async function checkChoices(root: string): Promise<void> {
    const checks = Object.entries(chunkChoices).flatMap(([source, choices]) =>
      Object.entries(choices).map(async ([relative, choice]) => {
        if (await stillSame(root, source, relative, choice)) return;
        if (chunkChoices[source]?.[relative] !== choice) return;
        dropChoice(source, relative);
        const status = project(source)?.files.find((file) => file.relative === relative)?.status;
        setTick(source, relative, status === "replaced");
      }),
    );
    await Promise.all(checks);
  }

  function commitPlan(next: ReceivePlan, meta: ReceiveSession, keep: boolean): void {
    const previous = activeSource.value;
    if (keep) keepPlan(next);
    else resetPlan(next);
    session.value = meta;
    if (previous && tabs.value.some((entry) => entry.source === previous)) activeSource.value = previous;
  }

  async function downloadClipboard(sequence: number, request: number): Promise<ClipboardDownload> {
    downloadCall += 1;
    const call = downloadCall;
    const current = () => downloading.value && request === analyzeRequest;
    downloading.value = true;
    downloadShown.value = false;
    downloadProgress.value = null;
    const reveal = setTimeout(() => {
      if (call === downloadCall) downloadShown.value = true;
    }, showDownloadAfterMs);
    try {
      return await api.downloadClipboard(sequence, (progress) => {
        if (!current()) return;
        downloadProgress.value = progress;
        downloadShown.value = true;
      });
    } finally {
      clearTimeout(reveal);
      if (call === downloadCall) {
        downloading.value = false;
        downloadShown.value = false;
        downloadProgress.value = null;
      }
    }
  }

  function downloadedRequests(
    requests: ReceiveRequest[],
    folders: readonly DownloadedFolder[],
    shown: readonly ClipboardEntry[],
  ): ReceiveRequest[] {
    const mapped = toDownloadedRequests(requests, folders);
    const missing = mapped.missing[0];
    if (missing === undefined) return mapped.requests;

    const name = shown.find((entry) => entry.path === missing)?.name ?? missing;
    throw i18n.global.t("transfer.notDownloaded", { name });
  }

  async function planFor(root: string, input: AnalyzeInput, request: number): Promise<FoundPlan | null> {
    const { source, sequence } = clipboard.value;
    const shown = entries.value;
    if (source !== "virtual") {
      return { plan: await api.analyzeReceive(root, input.requests, input.patterns, input.disabled), download: null };
    }
    const done = sequence === null ? null : await downloadClipboard(sequence, request);
    if (request !== analyzeRequest) return null;
    const requests = downloadedRequests(input.requests, done?.folders ?? [], shown);
    return { plan: await api.analyzeReceive(root, requests, input.patterns, input.disabled), download: done };
  }

  function currentInput(): AnalyzeInput {
    return {
      requests: requests.value,
      patterns: parsePatterns(receivePatterns.value),
      disabled: receiveDisabled.value,
    };
  }

  async function analyze(root: string): Promise<void> {
    await receiveAction.run(async () => {
      const input = currentInput();
      const signature = liveSignature.value;
      const keep = session.value?.signature === signature;
      analyzeRequest += 1;
      const request = analyzeRequest;
      let found: FoundPlan | null;
      try {
        found = await planFor(root, input, request);
      } catch (cause) {
        if (request === analyzeRequest) throw cause;
        return;
      }
      if (found === null || request !== analyzeRequest) return;

      const { patterns, disabled } = input;
      commitPlan(found.plan, { signature, download: found.download, patterns, disabled }, keep);
      receiveStep.value = "review";
      await checkChoices(root);
    });
  }

  function toggleReceivePattern(pattern: string): void {
    receiveDisabled.value = toggled(receiveDisabled.value, pattern);
  }

  async function refreshPlan(root: string): Promise<void> {
    const current = session.value;
    const shown = (plan.value?.projects ?? []).map(({ source, target }) => ({ source, target }));
    if (!current || !shown.length) return;
    analyzeRequest += 1;
    const request = analyzeRequest;
    const next = await api.analyzeReceive(root, shown, current.patterns, current.disabled).catch(() => null);
    if (!next || request !== analyzeRequest) return;
    commitPlan(next, current, true);
    await checkChoices(root);
  }

  function endSession(): void {
    analyzeRequest += 1;
    session.value = null;
    resetPlan(null);
    receiveAction.error.value = "";
    receiveStep.value = "clipboard";
  }

  function isSelected(source: string, relative: string): boolean {
    return selectedFor(source).has(relative);
  }

  function setTick(source: string, relative: string, value: boolean): void {
    if (isSelected(source, relative) === value) return;
    const next = new Set(selectedFor(source));
    if (value) next.add(relative);
    else next.delete(relative);
    selected[source] = next;
  }

  function setFiles(source: string, relatives: readonly string[], value: boolean): void {
    const next = new Set(selectedFor(source));
    for (const relative of relatives) {
      dropChoice(source, relative);
      if (value) next.add(relative);
      else next.delete(relative);
    }
    selected[source] = next;
  }

  function chunkChoiceFor(source: string, relative: string): ChunkChoice | null {
    return choicesFor(source)[relative] ?? null;
  }

  function mixedFor(source: string): ReadonlySet<string> {
    return new Set(Object.keys(choicesFor(source)));
  }

  function isDefaultTick(source: string, relative: string): boolean {
    const found = project(source)?.files.find((entry) => entry.relative === relative);
    return !!found && selectionFor([found], "changes").length > 0;
  }

  function sameSides(choice: ChunkChoice, file: ChunkFile): boolean {
    return choice.localSha256 === file.localSha256 && choice.receivedSha256 === file.receivedSha256;
  }

  function tickOf(source: string, relative: string, file?: ChunkFile): { choice: ChunkChoice | null; ticked: boolean } {
    const choice = chunkChoiceFor(source, relative);
    if (choice && file && !sameSides(choice, file)) return { choice: null, ticked: isDefaultTick(source, relative) };
    return { choice, ticked: isSelected(source, relative) };
  }

  /** Pass `file` when its current chunks are known; a choice made on other contents is then ignored. */
  function fileState(source: string, relative: string, file?: ChunkFile): CheckState {
    const { choice, ticked } = tickOf(source, relative, file);
    if (choice) return "indeterminate";
    return ticked ? "checked" : "unchecked";
  }

  /** Pass `file` when its current chunks are known; a choice made on other contents is then ignored. */
  function isChunkTaken(source: string, relative: string, index: number, file?: ChunkFile): boolean {
    const { choice, ticked } = tickOf(source, relative, file);
    return choice ? choice.taken.has(index) : ticked;
  }

  /**
   * Takes or keeps the chunks at `indexes` of `file`; a file left with all or none taken goes back to a plain tick.
   * A saved choice made on other contents than `file` is dropped and the file's default tick is the starting point.
   */
  function setChunks(
    source: string,
    relative: string,
    file: ChunkFile,
    indexes: readonly number[],
    take: boolean,
  ): void {
    const total = file.chunks.length;
    const taken = new Set<number>();
    for (let index = 0; index < total; index += 1) if (isChunkTaken(source, relative, index, file)) taken.add(index);
    for (const index of indexes) {
      if (index < 0 || index >= total) continue;
      if (take) taken.add(index);
      else taken.delete(index);
    }

    if (taken.size === 0 || taken.size === total) {
      dropChoice(source, relative);
      setTick(source, relative, taken.size === total);
      return;
    }
    setTick(source, relative, false);
    chunkChoices[source] = { ...choicesFor(source), [relative]: { ...file, chunks: [...file.chunks], taken } };
  }

  function toggleFile(source: string, relative: string): void {
    setFiles(source, [relative], !isSelected(source, relative));
  }

  function selectFiles(source: string, mode: SelectionMode, onlyAffected: boolean): void {
    const found = project(source);
    if (!found) return;
    selected[source] = new Set(selectionFor(visibleFiles(found.files, onlyAffected), mode));
    chunkChoices[source] = {};
  }

  function untickIdentical(): void {
    for (const entry of plan.value?.projects ?? []) {
      const identical = entry.files.filter((file) => file.status === "identical").map((file) => file.relative);
      setFiles(entry.source, identical, false);
    }
  }

  function isExpanded(source: string, path: string): boolean {
    return expanded[source]?.has(path) ?? false;
  }

  function toggleFolder(source: string, path: string): void {
    const next = new Set(expanded[source] ?? []);
    if (next.has(path)) next.delete(path);
    else next.add(path);
    expanded[source] = next;
  }

  function previewFor(source: string): string | null {
    return previews[source] ?? null;
  }

  function setPreview(source: string, relative: string | null): void {
    if (relative === null) delete previews[source];
    else previews[source] = relative;
  }

  function readFile(root: string, source: string, relative: string): Promise<ReceiveFileContents> {
    const key = `${source}
${relative}`;
    const cached = fileCache.get(key);
    if (cached) {
      fileCache.delete(key);
      fileCache.set(key, cached);
      return cached;
    }

    const reading = api.readReceiveFile(root, source, project(source)?.target ?? "", relative);
    fileCache.set(key, reading);
    for (const oldest of fileCache.keys()) {
      if (fileCache.size <= fileCacheSize) break;
      fileCache.delete(oldest);
    }
    reading.catch(() => {
      if (fileCache.get(key) === reading) fileCache.delete(key);
    });
    return reading;
  }

  async function cancelDownload(): Promise<void> {
    try {
      await api.cancelClipboardDownload();
    } catch (cause) {
      receiveAction.error.value = describeError(cause);
    }
  }

  async function apply(root: string, source?: string): Promise<void> {
    const applied = projectsToApply(source)
      .map(selectionOf)
      .filter((selection) => selection.files.length > 0 || selection.delete.length > 0 || !!selection.merges);
    if (!applied.length) return;

    await receiveAction.run(async () => {
      try {
        const outcome = await api.applyReceive(root, applied);
        outcome.projects.forEach((result, index) => {
          const selection = applied[index];
          if (!selection) return;
          results[selection.source] = { result, receivedAt: outcome.receivedAt, logFile: outcome.logFile };
        });
      } catch (cause) {
        await refreshPlan(root);
        throw cause;
      }
    });
  }

  return {
    tab,
    copyProjects,
    copyPatterns,
    copyDisabled,
    preview,
    copyResult,
    copyBusy: copyAction.busy,
    copyError: copyAction.error,
    copySelectionEmpty,
    entries,
    targets,
    receivePatterns,
    receiveDisabled,
    plan,
    tabs,
    unchangedProjects,
    activeSource,
    results,
    receiveBusy: receiveAction.busy,
    receiveError: receiveAction.error,
    watching,
    clipboard,
    clipboardBusy,
    download,
    downloading,
    downloadShown,
    downloadProgress,
    receiveStep,
    hasSession,
    clipboardChanged,
    pendingCount,
    requests,
    selectedCount,
    selectedFor,
    countFor,
    removalsFor,
    setCopyProjects,
    toggleCopyProject,
    toggleCopyPattern,
    previewCopy,
    copy,
    startWatching,
    stopWatching,
    folderFor,
    setTarget,
    closeProject,
    setActive,
    setReceiveStep,
    analyze,
    endSession,
    cancelDownload,
    toggleReceivePattern,
    isSelected,
    fileState,
    mixedFor,
    chunkChoiceFor,
    isChunkTaken,
    setChunks,
    setFiles,
    toggleFile,
    selectFiles,
    untickIdentical,
    isExpanded,
    toggleFolder,
    previewFor,
    setPreview,
    readFile,
    apply,
  };
});
