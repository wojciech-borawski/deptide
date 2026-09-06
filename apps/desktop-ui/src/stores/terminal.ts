import { computed, ref } from "vue";
import { useLocalStorage } from "@vueuse/core";
import { defineStore } from "pinia";

import * as api from "@/api/commands";
import { useRunTracker } from "@/composables/useRunTracker";

const maxHistory = 30;

const historyKey = "deptide:terminal-history";

export const useTerminalStore = defineStore("terminal", () => {
  const tracker = useRunTracker();
  const projectNames = ref<string[]>([]);
  const command = ref("");
  const concurrency = ref(3);
  const history = useLocalStorage<string[]>(historyKey, []);

  const canStart = computed(
    () =>
      projectNames.value.length > 0 &&
      command.value.trim().length > 0 &&
      !tracker.isActive.value &&
      !tracker.starting.value,
  );

  function rememberCommand(line: string): void {
    history.value = [line, ...history.value.filter((entry) => entry !== line)].slice(0, maxHistory);
  }

  function setProjects(names: string[]): void {
    projectNames.value = [...new Set(names)];
  }

  function toggleProject(name: string): void {
    const next = new Set(projectNames.value);
    if (next.has(name)) next.delete(name);
    else next.add(name);
    projectNames.value = [...next];
  }

  async function start(root: string): Promise<boolean> {
    const line = command.value.trim();
    if (!line || !projectNames.value.length) return false;

    const outcome = await tracker.track(() =>
      api.startCommandRun(root, projectNames.value, line, Math.max(1, concurrency.value)),
    );
    if (outcome) rememberCommand(line);
    return outcome !== null;
  }

  return {
    ...tracker,
    projectNames,
    command,
    concurrency,
    history,
    canStart,
    setProjects,
    toggleProject,
    start,
  };
});
