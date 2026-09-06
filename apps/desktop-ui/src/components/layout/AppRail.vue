<script setup lang="ts">
import { computed } from "vue";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useI18n } from "vue-i18n";
import { useRoute } from "vue-router";

import AppIcon, { type IconName } from "@/components/ui/AppIcon.vue";
import { routeNames } from "@/router";
import { useRunStore } from "@/stores/run";
import { useTerminalStore } from "@/stores/terminal";
import { useUiStore } from "@/stores/ui";
import { useWorkspaceStore } from "@/stores/workspace";

interface RailItem {
  name: string;
  label: string;
  icon: IconName;
}

const workspace = useWorkspaceStore();
const run = useRunStore();
const terminal = useTerminalStore();
const ui = useUiStore();
const route = useRoute();
const { t } = useI18n();

const items = computed<RailItem[]>(() => [
  { name: routeNames.wizard, label: t("nav.update"), icon: "update" },
  { name: routeNames.run, label: t("nav.run"), icon: "play" },
  { name: routeNames.transfer, label: t("nav.transfer"), icon: "layers" },
  { name: routeNames.terminal, label: t("nav.terminal"), icon: "terminal" },
  { name: routeNames.history, label: t("nav.history"), icon: "history" },
  { name: routeNames.settings, label: t("nav.settings"), icon: "settings" },
]);

const disabled = computed(() => !workspace.isOpen);
const newerVersion = computed(() => (ui.update?.newer ? ui.update : null));

function isCurrent(name: string): boolean {
  return route.name === name;
}

async function openUpdate(): Promise<void> {
  if (newerVersion.value?.url) await openUrl(newerVersion.value.url);
}
</script>

<template>
  <nav class="rail">
    <div class="brand" :title="t('app.name')">
      <AppIcon name="package" :size="22" />
    </div>

    <router-link
      v-for="item in items"
      :key="item.name"
      :to="{ name: item.name }"
      class="rail-item"
      :class="{ current: isCurrent(item.name), disabled }"
      :aria-disabled="disabled"
      :tabindex="disabled ? -1 : 0"
    >
      <span class="icon-wrap">
        <AppIcon :name="item.icon" :size="20" />
        <span
          v-if="
            (item.name === routeNames.run && run.isActive) || (item.name === routeNames.terminal && terminal.isActive)
          "
          class="pulse"
        />
      </span>
      <span class="label">{{ item.label }}</span>
    </router-link>

    <div class="spacer" />

    <button
      v-if="newerVersion"
      class="rail-item update"
      type="button"
      :title="t('nav.updateAvailable', { version: newerVersion.latest })"
      @click="openUpdate"
    >
      <span class="icon-wrap"><AppIcon name="bolt" :size="20" /></span>
      <span class="label">{{ newerVersion.latest }}</span>
    </button>

    <router-link
      :to="{ name: routeNames.workspace }"
      class="rail-item"
      :class="{ current: isCurrent(routeNames.workspace) }"
      :title="workspace.root || t('nav.chooseWorkspace')"
    >
      <span class="icon-wrap"><AppIcon name="folder" :size="20" /></span>
      <span class="label">{{ t("nav.workspace") }}</span>
    </router-link>
  </nav>
</template>

<style scoped>
.rail {
  display: flex;
  flex-direction: column;
  align-items: stretch;
  background: var(--bg-rail);
  border-right: 1px solid var(--border);
  padding: 10px 0;
  gap: 4px;
}

.brand {
  display: grid;
  place-items: center;
  height: 44px;
  margin-bottom: 10px;
  color: var(--accent);
}

.rail-item {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  padding: 9px 2px;
  margin: 0 6px;
  min-width: 0;
  border-radius: var(--radius-sm);
  border: none;
  background: transparent;
  color: var(--text-muted);
  text-decoration: none;
  font: inherit;
  font-size: 10.5px;
  font-weight: 600;
  letter-spacing: 0.02em;
  cursor: pointer;
  transition:
    background 0.12s ease,
    color 0.12s ease;
}

.rail-item:hover {
  background: var(--bg-hover);
  color: var(--text);
}

.rail-item.current {
  background: var(--accent-soft);
  color: var(--accent);
}

.rail-item.disabled {
  opacity: 0.35;
  pointer-events: none;
}

.rail-item.update {
  color: var(--warn);
}

.label {
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.icon-wrap {
  position: relative;
  display: grid;
  place-items: center;
}

.pulse {
  position: absolute;
  top: -2px;
  right: -4px;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--running);
  box-shadow: 0 0 0 0 rgba(56, 189, 248, 0.6);
  animation: pulse 1.4s ease-out infinite;
}

@keyframes pulse {
  to {
    box-shadow: 0 0 0 8px rgba(56, 189, 248, 0);
  }
}

.spacer {
  flex: 1;
}
</style>
