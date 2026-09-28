<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import type { ReceiveProjectPlan } from "@/api/types";
import AppIcon from "@/components/ui/AppIcon.vue";
import { nextTabIndex } from "@/lib/receive-view";
import StatusPills from "./StatusPills.vue";

const props = defineProps<{ projects: ReceiveProjectPlan[]; active: string | null; done: string[] }>();
const emit = defineEmits<{ select: [source: string]; close: [source: string] }>();
const { t } = useI18n();

const activeIndex = computed(() => props.projects.findIndex((project) => project.source === props.active));
const focusIndex = computed(() => Math.max(activeIndex.value, 0));

function tabId(index: number): string {
  return `receive-tab-${index}`;
}

function moveFocus(event: KeyboardEvent, index: number): void {
  const next = nextTabIndex(index, event.key, props.projects.length);
  const project = next === null ? undefined : props.projects[next];
  if (next === null || !project) return;

  event.preventDefault();
  emit("select", project.source);
  const tablist = (event.currentTarget as HTMLElement).closest('[role="tablist"]');
  tablist?.querySelectorAll<HTMLElement>('[role="tab"]')[next]?.focus();
}
</script>

<template>
  <div class="project-tabs" role="tablist" :aria-label="t('transfer.projectTabs')">
    <div
      v-for="(project, index) in props.projects"
      :key="project.source"
      class="project-tab"
      :class="{ active: props.active === project.source }"
      role="presentation"
    >
      <button
        :id="tabId(index)"
        aria-controls="receive-tab-panel"
        class="tab-button"
        type="button"
        role="tab"
        :aria-selected="props.active === project.source"
        :tabindex="index === focusIndex ? 0 : -1"
        :title="project.source"
        @click="emit('select', project.source)"
        @keydown="moveFocus($event, index)"
      >
        <AppIcon v-if="props.done.includes(project.source)" name="check" :size="14" class="done" />
        <span class="name">{{ project.target }}</span>
        <StatusPills :counts="project" />
      </button>
      <button
        class="close"
        type="button"
        tabindex="-1"
        aria-hidden="true"
        :title="t('transfer.closeProject', { name: project.target })"
        @click="emit('close', project.source)"
      >
        <AppIcon name="close" :size="13" />
      </button>
    </div>
  </div>
  <div id="receive-tab-panel" role="tabpanel" :aria-labelledby="activeIndex >= 0 ? tabId(activeIndex) : undefined">
    <slot />
  </div>
</template>

<style scoped>
.project-tabs {
  display: flex;
  flex-wrap: wrap;
  gap: 2px;
  border-bottom: 1px solid var(--border);
}

.project-tab {
  display: inline-flex;
  align-items: center;
  border: 1px solid transparent;
  border-bottom: none;
  border-radius: var(--radius-sm) var(--radius-sm) 0 0;
  margin-bottom: -1px;
}

.project-tab:hover {
  background: var(--bg-hover);
}

.project-tab.active {
  background: var(--bg-panel);
  border-color: var(--border);
  box-shadow: inset 0 2px 0 var(--accent);
}

.tab-button {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 8px 4px 8px 12px;
  border: none;
  background: none;
  color: var(--text-muted);
  font: inherit;
  font-weight: 600;
  cursor: pointer;
}

.project-tab.active .tab-button {
  color: var(--text);
}

.done {
  color: var(--ok);
}

.close {
  display: grid;
  place-items: center;
  width: 22px;
  height: 22px;
  margin-right: 6px;
  padding: 0;
  border: none;
  border-radius: var(--radius-sm);
  background: none;
  color: var(--text-faint);
  cursor: pointer;
}

.close:hover {
  background: var(--bg-elevated);
  color: var(--text);
}
</style>
