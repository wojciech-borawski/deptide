<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import type { ProjectView } from "@/api/types";
import AppIcon from "@/components/ui/AppIcon.vue";
import ProjectKindIcon from "@/components/ui/ProjectKindIcon.vue";
import ProjectPath from "@/components/ui/ProjectPath.vue";
import { groupByFolder } from "@/lib/project-paths";
import { useUiStore } from "@/stores/ui";

const props = defineProps<{
  projects: ProjectView[];
  selected: readonly string[];
  disabled?: boolean;
  emptyText?: string;
}>();
const emit = defineEmits<{ toggle: [name: string] }>();
const ui = useUiStore();
const { t } = useI18n();

const grouped = computed(() => groupByFolder(props.projects, (project) => ui.displayPath(project)));

function isSelected(project: ProjectView): boolean {
  return props.selected.includes(project.name);
}
</script>

<template>
  <div class="list">
    <template v-for="group in grouped.groups" :key="group.folder ?? ''">
      <div
        v-if="group.folder !== null"
        class="group-head"
        :class="{ shared: group.items.length > 1 }"
        :title="t('projects.inFolder', { count: group.items.length, folder: group.folder }, group.items.length)"
      >
        <AppIcon name="folder" :size="13" />
        <span>{{ group.folder }}</span>
        <span class="count">{{ group.items.length }}</span>
      </div>
      <label
        v-for="{ item: project, rest } in group.items"
        :key="project.name"
        class="list-row pick"
        :class="{ selected: isSelected(project), unavailable: !project.exists }"
      >
        <span class="check">
          <input
            type="checkbox"
            :checked="isSelected(project)"
            :disabled="props.disabled || !project.exists"
            @change="emit('toggle', project.name)"
          />
        </span>
        <ProjectKindIcon :kind="project.kind" />
        <span class="details">
          <span class="row">
            <span class="name">{{ project.name }}</span>
            <slot name="badges" :project="project" />
          </span>
          <ProjectPath
            :prefix="grouped.prefix"
            :rest="rest"
            :separator="grouped.separator"
            :grouped="group.items.length > 1"
            :full-path="project.absolutePath"
          />
        </span>
        <span v-if="$slots.actions" class="actions">
          <slot name="actions" :project="project" />
        </span>
      </label>
    </template>
    <p v-if="!props.projects.length && props.emptyText" class="list-row muted">{{ props.emptyText }}</p>
  </div>
</template>

<style scoped>
.pick {
  cursor: pointer;
}

.pick.unavailable {
  opacity: 0.6;
  cursor: default;
}

.details {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
  flex: 1;
}

.name {
  font-weight: 600;
}

.actions {
  opacity: 0;
  transition: opacity 0.1s ease;
}

.pick:hover .actions {
  opacity: 1;
}
</style>
