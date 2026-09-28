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
  ReceiveFileContents,
  ReceivePlan,
  ReceiveProjectPlan,
  ReceiveProjectResult,
  ReceiveRequest,
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
  type RemovalGroup,
  type SelectionMode,
} from "@/lib/receive-view";

export type TransferTab = "copy" | "receive";

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

export type ClipboardStatus = Omit<ClipboardContents, "entries">;

const pollMs = 2000;

const showDownloadAfterMs = 300;

const fileCacheSize = 20;

const noSelection: ReadonlySet<string> = new Set();

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
  const expanded = reactive<Record<string, Set<string>>>({});
  const results = reactive<Record<string, ReceivedProject>>({});
  const previews = reactive<Record<string, string>>({});
  const fileCache = new Map<string, Promise<ReceiveFileContents>>();
  const receiveAction = useAsyncAction();
  const watching = ref(false);
  const clipboard = ref<ClipboardStatus>({ source: "empty", sequence: null, rejected: 0, problem: null });
  const clipboardBusy = ref(false);
  const download = ref<ClipboardDownload | null>(null);
  const downloading = ref(false);
  const downloadShown = ref(false);
  const downloadProgress = ref<ExtractProgress | null>(null);

  let watchedRoot = "";
  let lastSignature = "";
  let analyzeRequest = 0;
  let downloadCall = 0;
  let planPatterns: Omit<AnalyzeInput, "requests"> = { patterns: [], disabled: [] };

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
  const selectedCount = computed(() => pendingTabs.value.reduce((sum, project) => sum + countFor(project.source), 0));

  function selectedFor(source: string): ReadonlySet<string> {
    return selected[source] ?? noSelection;
  }

  function project(source: string): ReceiveProjectPlan | undefined {
    return plan.value?.projects.find((entry) => entry.source === source);
  }

  function countFor(source: string): number {
    const found = project(source);
    if (!found) return 0;
    const selection = toSelection(found, selectedFor(source));
    return selection.files.length + selection.delete.length;
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
    if (signature === lastSignature) return;

    lastSignature = signature;
    analyzeRequest += 1;
    entries.value = next;
    download.value = null;
    resetPlan(null);

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

  function clipboardPathFor(source: string): string {
    return download.value ? clipboardIdFor(source, download.value.folders) : source;
  }

  function setTarget(path: string, target: string): void {
    targets[path] = target;
    dropProject(folderFor(path) ?? path);
  }

  function closeProject(source: string): void {
    setTarget(clipboardPathFor(source), "");
  }

  function setActive(source: string): void {
    activeSource.value = source;
  }

  function keepPlan(next: ReceivePlan): void {
    const before = new Map((plan.value?.projects ?? []).map((entry) => [entry.source, entry]));
    const ticks = { ...selected };
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
    }
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
      const done = await api.downloadClipboard(sequence, (progress) => {
        if (!current()) return;
        downloadProgress.value = progress;
        downloadShown.value = true;
      });
      if (request === analyzeRequest) download.value = done;
      return done;
    } finally {
      clearTimeout(reveal);
      if (call === downloadCall) {
        downloading.value = false;
        downloadShown.value = false;
        downloadProgress.value = null;
      }
    }
  }

  function downloadedRequests(requests: ReceiveRequest[], folders: readonly DownloadedFolder[]): ReceiveRequest[] {
    const mapped = toDownloadedRequests(requests, folders);
    const missing = mapped.missing[0];
    if (missing === undefined) return mapped.requests;

    download.value = null;
    const name = entries.value.find((entry) => entry.path === missing)?.name ?? missing;
    throw i18n.global.t("transfer.notDownloaded", { name });
  }

  async function planFor(root: string, input: AnalyzeInput, request: number): Promise<ReceivePlan | null> {
    let requests = input.requests;
    const { source, sequence } = clipboard.value;
    if (source === "virtual") {
      const done = sequence === null ? null : await downloadClipboard(sequence, request);
      if (request !== analyzeRequest) return null;
      requests = downloadedRequests(requests, done?.folders ?? []);
    }
    return api.analyzeReceive(root, requests, input.patterns, input.disabled);
  }

  async function fetchPlan(root: string, input: AnalyzeInput, keep: boolean): Promise<void> {
    analyzeRequest += 1;
    const request = analyzeRequest;
    let next: ReceivePlan | null;
    try {
      next = await planFor(root, input, request);
    } catch (cause) {
      if (request === analyzeRequest) throw cause;
      return;
    }
    if (next === null || request !== analyzeRequest) return;

    planPatterns = { patterns: input.patterns, disabled: input.disabled };
    const previous = activeSource.value;
    if (keep) keepPlan(next);
    else resetPlan(next);
    if (previous && tabs.value.some((entry) => entry.source === previous)) activeSource.value = previous;
  }

  function currentInput(): AnalyzeInput {
    return {
      requests: requests.value,
      patterns: parsePatterns(receivePatterns.value),
      disabled: receiveDisabled.value,
    };
  }

  async function analyze(root: string): Promise<void> {
    await receiveAction.run(() => fetchPlan(root, currentInput(), false));
  }

  async function toggleReceivePattern(root: string, pattern: string): Promise<void> {
    receiveDisabled.value = toggled(receiveDisabled.value, pattern);
    if (plan.value) await receiveAction.run(() => fetchPlan(root, currentInput(), true));
  }

  async function refreshPlan(root: string): Promise<void> {
    const shown = (plan.value?.projects ?? []).map(({ source, target }) => ({
      source: clipboardPathFor(source),
      target,
    }));
    if (!shown.length) return;
    await fetchPlan(root, { requests: shown, ...planPatterns }, true).catch(() => undefined);
  }

  function isSelected(source: string, relative: string): boolean {
    return selectedFor(source).has(relative);
  }

  function setFiles(source: string, relatives: readonly string[], value: boolean): void {
    const next = new Set(selectedFor(source));
    for (const relative of relatives) {
      if (value) next.add(relative);
      else next.delete(relative);
    }
    selected[source] = next;
  }

  function toggleFile(source: string, relative: string): void {
    setFiles(source, [relative], !isSelected(source, relative));
  }

  function selectFiles(source: string, mode: SelectionMode, onlyAffected: boolean): void {
    const found = project(source);
    if (found) selected[source] = new Set(selectionFor(visibleFiles(found.files, onlyAffected), mode));
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
      .map((entry) => toSelection(entry, selectedFor(entry.source)))
      .filter((selection) => selection.files.length > 0 || selection.delete.length > 0);
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
    analyze,
    cancelDownload,
    toggleReceivePattern,
    isSelected,
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
