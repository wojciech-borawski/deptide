<script setup lang="ts">
import { computed, onMounted } from "vue";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";

import ProjectPickerList from "@/components/projects/ProjectPickerList.vue";
import AppIcon from "@/components/ui/AppIcon.vue";
import ActionButton from "@/components/ui/ActionButton.vue";
import NoticeBanner from "@/components/ui/NoticeBanner.vue";
import { formatBytes, formatDateTime } from "@/lib/format";
import { routeNames } from "@/router";
import { useTransferStore } from "@/stores/transfer";
import { useWizardStore } from "@/stores/wizard";
import { useWorkspaceStore } from "@/stores/workspace";

const workspace = useWorkspaceStore();
const wizard = useWizardStore();
const transfer = useTransferStore();
const router = useRouter();
const { t } = useI18n();

const projects = computed(() => workspace.projects.filter((project) => project.exists));

const globalPatterns = computed(() => workspace.config?.transferIgnore ?? []);
const allSelected = computed(
  () => projects.value.length > 0 && projects.value.every((project) => transfer.copyProjects.includes(project.name)),
);

function toggleAll(): void {
  transfer.setCopyProjects(allSelected.value ? [] : projects.value.map((project) => project.name));
}

function goToSettings(): void {
  void router.push({ name: routeNames.settings });
}

onMounted(() => {
  if (!transfer.copyProjects.length && wizard.draft.projectNames.length) {
    transfer.setCopyProjects(
      wizard.draft.projectNames.filter((name) => projects.value.some((project) => project.name === name)),
    );
  }
});
</script>

<template>
  <div class="stack">
    <p class="muted">{{ t("transfer.copyIntro") }}</p>

    <div class="grid-2">
      <section class="card stack">
        <div class="card-title">
          <h3>{{ t("transfer.projects") }}</h3>
          <button class="btn btn-sm" type="button" :disabled="!projects.length" @click="toggleAll">
            {{ allSelected ? t("projects.deselectAll") : t("projects.selectAll") }}
          </button>
        </div>
        <ProjectPickerList
          class="scroll"
          :projects="projects"
          :selected="transfer.copyProjects"
          @toggle="transfer.toggleCopyProject"
        />
      </section>

      <section class="card stack">
        <div class="field">
          <label>{{ t("transfer.ignoreGlobal") }}</label>
          <div class="chips">
            <span v-for="pattern in globalPatterns" :key="pattern" class="chip mono">{{ pattern }}</span>
            <span v-if="!globalPatterns.length" class="faint">-</span>
            <button class="chip" type="button" @click="goToSettings">
              <AppIcon name="settings" :size="12" />
              {{ t("nav.settings") }}
            </button>
          </div>
          <span class="hint">{{ t("transfer.ignoreGlobalHint") }}</span>
        </div>
        <div class="field">
          <label>{{ t("transfer.ignoreRun") }}</label>
          <textarea
            v-model="transfer.copyPatterns"
            class="textarea mono"
            rows="4"
            placeholder="coverage/&#10;*.log"
          ></textarea>
          <span class="hint">{{ t("transfer.ignoreRunHint") }}</span>
        </div>
      </section>
    </div>

    <div class="toolbar">
      <button
        class="btn"
        type="button"
        :disabled="transfer.copySelectionEmpty || transfer.copyBusy"
        @click="transfer.previewCopy(workspace.root)"
      >
        <AppIcon name="search" :size="15" />
        {{ t("transfer.preview") }}
      </button>
      <ActionButton
        variant="primary"
        icon="layers"
        :busy="transfer.copyBusy"
        :disabled="transfer.copySelectionEmpty"
        @click="transfer.copy(workspace.root)"
      >
        {{ t("transfer.copy") }}
      </ActionButton>
      <span v-if="transfer.copySelectionEmpty" class="muted">{{ t("transfer.noneSelected") }}</span>
    </div>

    <NoticeBanner v-if="transfer.copyError" tone="error" selectable>{{ transfer.copyError }}</NoticeBanner>

    <section v-if="transfer.preview" class="card">
      <div class="card-title">
        <h3>{{ t("transfer.previewTitle") }}</h3>
        <span class="muted">{{
          t("transfer.filesAndSize", { files: transfer.preview.files, size: formatBytes(transfer.preview.bytes) })
        }}</span>
      </div>
      <table class="summary">
        <tbody>
          <tr v-for="project in transfer.preview.projects" :key="project.name">
            <td class="name">{{ project.name }}</td>
            <td class="mono">{{ project.files }}</td>
            <td class="mono">{{ formatBytes(project.bytes) }}</td>
            <td class="muted">
              {{ project.skipped ? t("transfer.skippedByPatterns", { count: project.skipped }) : "" }}
            </td>
          </tr>
        </tbody>
      </table>
      <p v-if="transfer.preview.patterns.length" class="mono muted small">{{ transfer.preview.patterns.join("  ") }}</p>
    </section>

    <section v-if="transfer.copyResult" class="card">
      <div class="card-title">
        <h3 class="ok">{{ t("transfer.copiedTitle") }}</h3>
        <span class="muted"
          >{{ formatDateTime(transfer.copyResult.copiedAt) }} · {{ formatBytes(transfer.copyResult.bytes) }}</span
        >
      </div>
      <table class="summary">
        <tbody>
          <tr v-for="project in transfer.copyResult.projects" :key="project.name">
            <td class="name">{{ project.name }}</td>
            <td class="mono">{{ project.files }}</td>
            <td class="mono">{{ formatBytes(project.bytes) }}</td>
            <td class="muted">
              {{ project.skipped ? t("transfer.skippedByPatterns", { count: project.skipped }) : "" }}
            </td>
          </tr>
        </tbody>
      </table>
      <p class="muted small selectable">{{ t("transfer.stagedAt", { path: transfer.copyResult.stagingDirectory }) }}</p>
      <p v-if="transfer.copyResult.logFile" class="muted small selectable">
        {{ t("transfer.logWritten", { path: transfer.copyResult.logFile }) }}
      </p>
    </section>
  </div>
</template>

<style scoped>
.scroll {
  max-height: 320px;
  overflow: auto;
}

.chips {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.summary {
  width: 100%;
  border-collapse: collapse;
  font-size: 13px;
}

.summary td {
  padding: 6px 8px;
  border-bottom: 1px solid var(--border);
}

.summary tr:last-child td {
  border-bottom: none;
}

.small {
  font-size: 12px;
  margin-top: 8px;
}

.ok {
  color: var(--ok);
  text-transform: none;
  font-size: 15px;
  letter-spacing: 0;
}
</style>
