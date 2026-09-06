<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";

import type { ProjectView } from "@/api/types";
import ScanPanel from "./ScanPanel.vue";
import ProjectKindChips from "@/components/projects/ProjectKindChips.vue";
import ProjectPickerList from "@/components/projects/ProjectPickerList.vue";
import AppIcon from "@/components/ui/AppIcon.vue";
import EmptyState from "@/components/ui/EmptyState.vue";
import ModalDialog from "@/components/ui/ModalDialog.vue";
import SearchBox from "@/components/ui/SearchBox.vue";
import NoticeBanner from "@/components/ui/NoticeBanner.vue";
import { useCountedNoun } from "@/composables/useCountedNoun";
import { useProjectFilter } from "@/composables/useProjectFilter";
import { useProjectSelection } from "@/composables/useProjectSelection";
import { useWizardStore } from "@/stores/wizard";
import { useWorkspaceStore } from "@/stores/workspace";

const workspace = useWorkspaceStore();
const wizard = useWizardStore();
const { t } = useI18n();
const countOf = useCountedNoun();

const scanOpen = ref(false);
const removing = ref<ProjectView | null>(null);
const notice = ref("");

const filter = useProjectFilter(computed(() => workspace.projects));
const selection = useProjectSelection(
  { names: () => wizard.draft.projectNames, replace: wizard.setProjects },
  filter.visible,
);

async function confirmRemove(): Promise<void> {
  const target = removing.value;
  const config = workspace.config;
  if (!target || !config) return;

  await workspace.updateConfig({
    ...config,
    projects: config.projects.filter((project) => project.name !== target.name),
  });
  wizard.setProjects(wizard.draft.projectNames.filter((name) => name !== target.name));
  removing.value = null;
}

function onScanApplied(summary: { added: string[]; removed: string[] }): void {
  const parts: string[] = [];
  if (summary.added.length) {
    parts.push(t("projects.added", { count: countOf("project", summary.added.length) }));
  }
  if (summary.removed.length) {
    parts.push(
      t("projects.removed", {
        count: countOf("project", summary.removed.length),
      }),
    );
  }
  notice.value = parts.length
    ? t("projects.configUpdated", { parts: parts.join(", ") })
    : t("projects.configUnchanged");

  const known = new Set(workspace.projects.map((project) => project.name));
  wizard.setProjects([...wizard.draft.projectNames.filter((name) => known.has(name)), ...summary.added]);
}
</script>

<template>
  <div class="stack">
    <div class="toolbar">
      <SearchBox v-model="filter.text.value" :placeholder="t('projects.filter')" width="280px" />
      <button
        class="btn btn-sm"
        type="button"
        :disabled="!selection.selectable.value.length"
        @click="selection.toggleAllVisible"
      >
        {{ selection.allVisibleSelected.value ? t("projects.deselectAll") : t("projects.selectAll") }}
      </button>
      <span class="muted">{{
        t("common.ofSelected", { selected: wizard.draft.projectNames.length, total: workspace.projects.length })
      }}</span>
      <ProjectKindChips v-model="filter.kind.value" />
      <span class="spacer" />
      <button class="btn btn-sm" type="button" @click="scanOpen = true">
        <AppIcon name="search" :size="15" />
        {{ t("projects.scan") }}
      </button>
    </div>

    <NoticeBanner v-if="notice" tone="ok">{{ notice }}</NoticeBanner>

    <EmptyState
      v-if="!workspace.projects.length"
      icon="layers"
      :title="t('projects.noneConfigured')"
      :text="t('projects.noneConfiguredText')"
    >
      <button class="btn btn-primary" type="button" @click="scanOpen = true">{{ t("projects.scan") }}</button>
    </EmptyState>

    <ProjectPickerList
      v-else
      :projects="filter.visible.value"
      :selected="wizard.draft.projectNames"
      :empty-text="t('projects.noMatch')"
      @toggle="selection.toggle"
    >
      <template #badges="{ project }">
        <span v-if="!project.exists" class="badge badge-failed">{{ t("projects.folderMissing") }}</span>
        <span
          v-if="project.duplicateOf.length"
          class="badge badge-warn"
          :title="t('projects.duplicateTitle', { names: project.duplicateOf.join(', ') })"
        >
          {{ t("projects.duplicate") }}
        </span>
        <span v-if="project.skip" class="badge">{{ t("projects.skippedByDefault") }}</span>
        <span v-if="project.packages?.length" class="badge badge-violet" :title="project.packages.join(', ')">
          {{
            t("projects.packagesOnly", {
              count: countOf("package", project.packages.length),
            })
          }}
        </span>
      </template>
      <template #actions="{ project }">
        <button
          class="btn btn-ghost btn-icon"
          type="button"
          :title="t('projects.removeFromConfig')"
          @click.prevent="removing = project"
        >
          <AppIcon name="trash" :size="15" />
        </button>
      </template>
    </ProjectPickerList>

    <ScanPanel :open="scanOpen" @close="scanOpen = false" @applied="onScanApplied" />

    <ModalDialog :open="removing !== null" :title="t('projects.removeTitle')" width="460px" @close="removing = null">
      <p>{{ t("projects.removeText", { name: removing?.name ?? "" }) }}</p>
      <template #footer>
        <button class="btn" type="button" @click="removing = null">{{ t("common.cancel") }}</button>
        <button class="btn btn-danger" type="button" @click="confirmRemove">{{ t("common.remove") }}</button>
      </template>
    </ModalDialog>
  </div>
</template>
