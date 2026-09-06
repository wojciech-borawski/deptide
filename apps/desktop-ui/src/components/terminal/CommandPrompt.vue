<script setup lang="ts">
import { ref } from "vue";
import { useI18n } from "vue-i18n";

import ActionButton from "@/components/ui/ActionButton.vue";
import { useTerminalStore } from "@/stores/terminal";
import { useWorkspaceStore } from "@/stores/workspace";

const workspace = useWorkspaceStore();
const terminal = useTerminalStore();
const { t } = useI18n();

const historyIndex = ref(-1);
const commandInput = ref<HTMLInputElement | null>(null);

async function submit(): Promise<void> {
  if (!terminal.canStart) return;
  historyIndex.value = -1;
  await terminal.start(workspace.root);
}

function recall(direction: 1 | -1): void {
  const entries = terminal.history;
  if (!entries.length) return;
  const next = Math.min(entries.length - 1, Math.max(-1, historyIndex.value + direction));
  historyIndex.value = next;
  terminal.command = next < 0 ? "" : (entries[next] ?? "");
}

function useHistory(line: string): void {
  terminal.command = line;
  commandInput.value?.focus();
}
</script>

<template>
  <form class="prompt card" @submit.prevent="submit">
    <span class="prompt-sign mono">$</span>
    <input
      ref="commandInput"
      v-model="terminal.command"
      class="input mono command"
      :placeholder="t('terminal.placeholder')"
      :disabled="terminal.isActive"
      autocomplete="off"
      spellcheck="false"
      @keydown.up.prevent="recall(1)"
      @keydown.down.prevent="recall(-1)"
    />
    <span class="prompt-controls">
      <label class="concurrency" :title="t('terminal.parallelTitle')">
        <span class="muted">{{ t("terminal.parallel") }}</span>
        <input
          v-model.number="terminal.concurrency"
          class="input input-sm mono"
          type="number"
          min="1"
          max="32"
          :disabled="terminal.isActive"
        />
      </label>
      <ActionButton
        variant="primary"
        type="submit"
        icon="play"
        :busy="terminal.starting"
        :disabled="!terminal.canStart"
      >
        {{ t("terminal.run") }}
      </ActionButton>
    </span>
  </form>

  <p class="hint">
    {{
      terminal.projectNames.length
        ? t("terminal.hint", { count: terminal.projectNames.length }, terminal.projectNames.length)
        : t("terminal.pickProjects")
    }}
  </p>

  <div v-if="terminal.history.length && !terminal.snapshot" class="history">
    <span class="muted">{{ t("terminal.recent") }}</span>
    <button
      v-for="line in terminal.history.slice(0, 8)"
      :key="line"
      class="chip mono"
      type="button"
      @click="useHistory(line)"
    >
      {{ line }}
    </button>
  </div>
</template>

<style scoped>
.prompt {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 10px;
  padding: 10px 14px;
}

.prompt-controls {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-left: auto;
}

.prompt-sign {
  color: var(--accent);
  font-weight: 700;
  font-size: 16px;
}

.command {
  flex: 1 1 220px;
  min-width: 0;
  font-size: 14px;
}

.concurrency {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 12.5px;
}

.concurrency .input {
  width: 64px;
}

.hint {
  margin: -4px 0 0;
  font-size: 12.5px;
  color: var(--text-muted);
}

.history {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  font-size: 12.5px;
}
</style>
