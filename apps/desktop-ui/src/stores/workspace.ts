import { computed, ref } from "vue";
import { StorageSerializers, useLocalStorage } from "@vueuse/core";
import { defineStore } from "pinia";

import * as api from "@/api/commands";
import { describeError, useAsyncAction } from "@/composables/useAsyncAction";
import type { RecentWorkspace, Settings, UpdateConfig, WorkspaceSnapshot } from "@/api/types";

const lastWorkspaceKey = "deptide:last-workspace";

export const useWorkspaceStore = defineStore("workspace", () => {
  const snapshot = ref<WorkspaceSnapshot | null>(null);
  const lastWorkspace = useLocalStorage<string | null>(lastWorkspaceKey, null, {
    serializer: StorageSerializers.string,
  });
  const recent = ref<RecentWorkspace[]>([]);
  const openAction = useAsyncAction();
  const loading = openAction.busy;
  const error = openAction.error;

  const isOpen = computed(() => snapshot.value !== null);
  const root = computed(() => snapshot.value?.root ?? "");
  const settings = computed(() => snapshot.value?.settings ?? null);
  const config = computed(() => snapshot.value?.config ?? null);
  const projects = computed(() => snapshot.value?.projects ?? []);
  const savedRuns = computed(() => snapshot.value?.savedRuns ?? []);
  const history = computed(() => snapshot.value?.history ?? []);

  async function loadRecent(): Promise<void> {
    try {
      recent.value = await api.listRecentWorkspaces();
    } catch (cause) {
      error.value = describeError(cause);
    }
  }

  async function open(path: string): Promise<boolean> {
    const opened = await openAction.run(async () => {
      snapshot.value = await api.openWorkspace(path);
      recent.value = snapshot.value.recent;
      lastWorkspace.value = snapshot.value.root;
    });
    return opened !== undefined;
  }

  async function refresh(): Promise<void> {
    if (!snapshot.value) return;
    await open(snapshot.value.root);
  }

  async function updateSettings(next: Settings): Promise<void> {
    if (!snapshot.value) return;
    const saved = await api.saveSettings(snapshot.value.root, next);
    snapshot.value = { ...snapshot.value, settings: saved };
  }

  async function updateConfig(next: UpdateConfig): Promise<void> {
    if (!snapshot.value) return;
    snapshot.value = await api.saveConfig(snapshot.value.root, next);
  }

  function close(): void {
    snapshot.value = null;
    lastWorkspace.value = null;
  }

  return {
    snapshot,
    recent,
    loading,
    error,
    isOpen,
    root,
    settings,
    config,
    projects,
    savedRuns,
    history,
    lastWorkspace,
    loadRecent,
    open,
    refresh,
    updateSettings,
    updateConfig,
    close,
  };
});
