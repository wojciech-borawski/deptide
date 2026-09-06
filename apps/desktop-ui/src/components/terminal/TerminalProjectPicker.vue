<script setup lang="ts">
import { computed, watch } from "vue";
import { useI18n } from "vue-i18n";

import ProjectKindChips from "@/components/projects/ProjectKindChips.vue";
import ProjectPickerList from "@/components/projects/ProjectPickerList.vue";
import SearchBox from "@/components/ui/SearchBox.vue";
import { useProjectFilter } from "@/composables/useProjectFilter";
import { useProjectSelection } from "@/composables/useProjectSelection";
import { useTerminalStore } from "@/stores/terminal";
import { useWizardStore } from "@/stores/wizard";
import { useWorkspaceStore } from "@/stores/workspace";

const workspace = useWorkspaceStore();
const wizard = useWizardStore();
const terminal = useTerminalStore();
const { t } = useI18n();

const projects = computed(() => workspace.projects.filter((project) => project.exists));
const filter = useProjectFilter(projects);
const selection = useProjectSelection(
  { names: () => terminal.projectNames, replace: terminal.setProjects },
  filter.visible,
);

function defaultSelection(): string[] {
  const available = new Set(projects.value.map((project) => project.name));
  const fromWizard = wizard.draft.projectNames.filter((name) => available.has(name));
  if (fromWizard.length) return fromWizard;
  return projects.value.filter((project) => !project.skip).map((project) => project.name);
}

watch(
  projects,
  () => {
    if (!terminal.projectNames.length && projects.value.length) terminal.setProjects(defaultSelection());
  },
  { immediate: true },
);
</script>

<template>
  <section class="card picker">
    <div class="card-title">
      <h3>{{ t("terminal.projects") }}</h3>
      <span class="muted">{{
        t("common.ofSelected", { selected: terminal.projectNames.length, total: projects.length })
      }}</span>
    </div>
    <div class="toolbar compact">
      <SearchBox v-model="filter.text.value" :placeholder="t('projects.filter')" />
      <button
        class="btn btn-sm"
        type="button"
        :disabled="!selection.selectable.value.length"
        @click="selection.toggleAllVisible"
      >
        {{ selection.allVisibleSelected.value ? t("projects.deselectAll") : t("projects.selectAll") }}
      </button>
    </div>
    <ProjectKindChips v-model="filter.kind.value" />
    <ProjectPickerList
      class="scroll"
      :projects="filter.visible.value"
      :selected="terminal.projectNames"
      :disabled="terminal.isActive"
      :empty-text="t('terminal.noProjects')"
      @toggle="selection.toggle"
    />
  </section>
</template>

<style scoped>
.picker {
  display: flex;
  flex-direction: column;
  gap: 10px;
  position: sticky;
  top: 0;
}

.compact {
  gap: 8px;
}

.compact > :first-child {
  flex: 1;
}

.scroll {
  max-height: calc(100vh - 320px);
  overflow: auto;
}

@container (max-width: 760px) {
  .picker {
    position: static;
  }

  .scroll {
    max-height: 220px;
  }
}
</style>
