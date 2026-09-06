import { computed, reactive, ref } from "vue";
import { useIntervalFn } from "@vueuse/core";
import { defineStore } from "pinia";

import * as api from "@/api/commands";
import type {
  ClipboardEntry,
  CopyResult,
  ReceivePlan,
  ReceiveResult,
  ReceiveSelection,
  TransferPreview,
} from "@/api/types";
import { describeError, useAsyncAction } from "@/composables/useAsyncAction";

export type TransferTab = "copy" | "receive";

const pollMs = 2000;

function parsePatterns(text: string): string[] {
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
}

export const useTransferStore = defineStore("transfer", () => {
  const tab = ref<TransferTab>("copy");

  const copyProjects = ref<string[]>([]);
  const copyPatterns = ref("");
  const preview = ref<TransferPreview | null>(null);
  const copyResult = ref<CopyResult | null>(null);
  const copyAction = useAsyncAction();

  const entries = ref<ClipboardEntry[]>([]);
  const targets = reactive<Record<string, string>>({});
  const receivePatterns = ref("");
  const plan = ref<ReceivePlan | null>(null);
  const selected = reactive<Record<string, string[]>>({});
  const receiveResult = ref<ReceiveResult | null>(null);
  const receiveAction = useAsyncAction();
  const watching = ref(false);

  let watchedRoot = "";
  let lastSignature = "";

  const copySelectionEmpty = computed(() => copyProjects.value.length === 0);
  const requests = computed(() =>
    entries.value
      .filter((entry) => targets[entry.path])
      .map((entry) => ({ source: entry.path, target: targets[entry.path] ?? "" })),
  );
  const selections = computed<ReceiveSelection[]>(() =>
    (plan.value?.projects ?? [])
      .map((project) => ({
        source: project.source,
        target: project.target,
        files: selected[project.source] ?? [],
      }))
      .filter((selection) => selection.files.length > 0),
  );
  const selectedCount = computed(() => selections.value.reduce((sum, entry) => sum + entry.files.length, 0));

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

  async function previewCopy(root: string): Promise<void> {
    await copyAction.run(async () => {
      preview.value = await api.previewTransfer(root, copyProjects.value, parsePatterns(copyPatterns.value));
    });
  }

  async function copy(root: string): Promise<void> {
    await copyAction.run(async () => {
      copyResult.value = await api.copyProjectsToClipboard(root, copyProjects.value, parsePatterns(copyPatterns.value));
      preview.value = null;
    });
  }

  function applyEntries(next: ClipboardEntry[]): void {
    const signature = next.map((entry) => entry.path).join("|");
    if (signature === lastSignature) return;

    lastSignature = signature;
    entries.value = next;
    plan.value = null;
    receiveResult.value = null;

    for (const key of Object.keys(targets)) delete targets[key];
    for (const entry of next) targets[entry.path] = entry.suggestedProject ?? "";
  }

  async function poll(root: string): Promise<void> {
    try {
      const contents = await api.inspectClipboard(root);
      applyEntries(contents.entries);
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

  function setTarget(path: string, target: string): void {
    targets[path] = target;
    plan.value = null;
  }

  function selectChangedFiles(next: ReceivePlan): void {
    for (const key of Object.keys(selected)) delete selected[key];
    for (const project of next.projects) {
      selected[project.source] = project.files
        .filter((file) => file.status !== "identical")
        .map((file) => file.relative);
    }
  }

  async function analyze(root: string): Promise<void> {
    await receiveAction.run(async () => {
      const next = await api.analyzeReceive(root, requests.value, parsePatterns(receivePatterns.value));
      plan.value = next;
      receiveResult.value = null;
      selectChangedFiles(next);
    });
  }

  function isSelected(source: string, relative: string): boolean {
    return (selected[source] ?? []).includes(relative);
  }

  function toggleFile(source: string, relative: string): void {
    const current = selected[source] ?? [];
    selected[source] = current.includes(relative)
      ? current.filter((entry) => entry !== relative)
      : [...current, relative];
  }

  function selectFiles(source: string, mode: "all" | "none" | "changes"): void {
    const project = plan.value?.projects.find((entry) => entry.source === source);
    if (!project) return;

    if (mode === "none") selected[source] = [];
    else if (mode === "all") selected[source] = project.files.map((file) => file.relative);
    else selected[source] = project.files.filter((file) => file.status !== "identical").map((file) => file.relative);
  }

  async function apply(root: string): Promise<void> {
    await receiveAction.run(async () => {
      receiveResult.value = await api.applyReceive(root, selections.value);
      plan.value = null;
    });
  }

  return {
    tab,
    copyProjects,
    copyPatterns,
    preview,
    copyResult,
    copyBusy: copyAction.busy,
    copyError: copyAction.error,
    copySelectionEmpty,
    entries,
    targets,
    receivePatterns,
    plan,
    selected,
    receiveResult,
    receiveBusy: receiveAction.busy,
    receiveError: receiveAction.error,
    watching,
    requests,
    selections,
    selectedCount,
    setCopyProjects,
    toggleCopyProject,
    previewCopy,
    copy,
    startWatching,
    stopWatching,
    setTarget,
    analyze,
    isSelected,
    toggleFile,
    selectFiles,
    apply,
  };
});
