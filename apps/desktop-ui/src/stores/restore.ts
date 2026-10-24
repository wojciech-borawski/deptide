import { ref } from "vue";
import { defineStore } from "pinia";

import * as api from "@/api/commands";
import type { ProjectRestore, RunBackup, RunSummary } from "@/api/types";

export const useRestoreStore = defineStore("restore", () => {
  const backups = ref<RunBackup[]>([]);
  let latest = 0;

  async function load(root: string): Promise<void> {
    latest += 1;
    const request = latest;
    try {
      const answer = await api.listRunBackups(root);
      if (request !== latest) return;
      backups.value = answer;
    } catch (cause) {
      if (request !== latest) return;
      throw cause;
    }
  }

  function backupFor(summary: RunSummary | null): RunBackup | null {
    if (!summary?.logFile) return null;
    return backups.value.find((backup) => backup.logFile === summary.logFile) ?? null;
  }

  function restore(root: string, backup: RunBackup): Promise<ProjectRestore[]> {
    return api.restoreRunFiles(
      root,
      backup.run,
      backup.projects.map((project) => project.name),
    );
  }

  return { backups, load, backupFor, restore };
});
