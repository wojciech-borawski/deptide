<script setup lang="ts">
import { computed, onMounted, onUnmounted, watch } from "vue";
import { useI18n } from "vue-i18n";

import PageHeader from "@/components/layout/PageHeader.vue";
import DependencyGraphTab from "@/components/dependencies/DependencyGraphTab.vue";
import DuplicateCopiesTab from "@/components/dependencies/DuplicateCopiesTab.vue";
import VersionDriftTab from "@/components/dependencies/VersionDriftTab.vue";
import { nextTabIndex } from "@/lib/receive-view";
import { dependencyTabs, useDependenciesStore, type DependencyTab } from "@/stores/dependencies";
import { useWorkspaceStore } from "@/stores/workspace";

const workspace = useWorkspaceStore();
const dependencies = useDependenciesStore();
const { t } = useI18n();

const panels = { versions: VersionDriftTab, graph: DependencyGraphTab, duplicates: DuplicateCopiesTab };
const panel = computed(() => panels[dependencies.tab]);

function tabId(tab: DependencyTab): string {
  return `dependencies-tab-${tab}`;
}

function loadTab(tab: DependencyTab): void {
  if (tab === "versions" && !dependencies.drift && !dependencies.driftBusy) void dependencies.loadDrift(false);
  if (tab === "graph" && !dependencies.graph && !dependencies.graphBusy) void dependencies.loadGraph();
}

function select(tab: DependencyTab): void {
  dependencies.tab = tab;
  loadTab(tab);
}

function moveFocus(event: KeyboardEvent, index: number): void {
  const next = nextTabIndex(index, event.key, dependencyTabs.length);
  const tab = next === null ? undefined : dependencyTabs[next];
  if (!tab) return;

  event.preventDefault();
  select(tab);
  const tablist = (event.currentTarget as HTMLElement).closest('[role="tablist"]');
  tablist?.querySelectorAll<HTMLElement>('[role="tab"]')[next ?? 0]?.focus();
}

onMounted(() => {
  dependencies.openRoot(workspace.root);
  loadTab(dependencies.tab);
});

onUnmounted(() => dependencies.cancelAll());

watch(
  () => workspace.root,
  (root) => {
    dependencies.openRoot(root);
    loadTab(dependencies.tab);
  },
);
</script>

<template>
  <div class="page">
    <PageHeader :title="t('dependencies.title')" :subtitle="workspace.root" />

    <div class="tabs" role="tablist" :aria-label="t('dependencies.title')">
      <button
        v-for="(tab, index) in dependencyTabs"
        :id="tabId(tab)"
        :key="tab"
        class="tab"
        :class="{ active: dependencies.tab === tab }"
        type="button"
        role="tab"
        aria-controls="dependencies-panel"
        :aria-selected="dependencies.tab === tab"
        :tabindex="dependencies.tab === tab ? 0 : -1"
        @click="select(tab)"
        @keydown="moveFocus($event, index)"
      >
        {{ t(`dependencies.tabs.${tab}`) }}
      </button>
    </div>

    <div id="dependencies-panel" class="body" role="tabpanel" :aria-labelledby="tabId(dependencies.tab)">
      <component :is="panel" />
    </div>
  </div>
</template>

<style scoped>
.page {
  display: flex;
  flex-direction: column;
  height: 100%;
}

.tabs {
  display: flex;
  gap: 4px;
  padding: 0 24px;
  border-bottom: 1px solid var(--border);
}

.tab {
  padding: 9px 14px;
  border: none;
  border-bottom: 2px solid transparent;
  background: transparent;
  color: var(--text-muted);
  font: inherit;
  font-weight: 600;
  cursor: pointer;
}

.tab:hover {
  color: var(--text);
}

.tab.active {
  color: var(--accent);
  border-bottom-color: var(--accent);
}

.tab:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: -2px;
}

.body {
  flex: 1;
  overflow: auto;
  padding: 20px 24px;
}
</style>
