<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted } from "vue";
import { useI18n } from "vue-i18n";

import type { ReceiveProjectPlan } from "@/api/types";
import AppIcon from "@/components/ui/AppIcon.vue";
import ActionButton from "@/components/ui/ActionButton.vue";
import NoticeBanner from "@/components/ui/NoticeBanner.vue";
import { formatBytes, formatDateTime } from "@/lib/format";
import { useTransferStore } from "@/stores/transfer";
import { useWorkspaceStore } from "@/stores/workspace";

const workspace = useWorkspaceStore();
const transfer = useTransferStore();
const { t } = useI18n();

const projectNames = computed(() =>
  workspace.projects.filter((project) => project.exists).map((project) => project.name),
);
const canAnalyze = computed(() => transfer.requests.length > 0 && !transfer.receiveBusy);
const canApply = computed(() => transfer.selectedCount > 0 && !transfer.receiveBusy);

function statusClass(status: string): string {
  if (status === "added") return "badge-ok";
  if (status === "replaced") return "badge-warn";
  return "badge-skipped";
}

function counts(project: ReceiveProjectPlan): string {
  return t("transfer.planCounts", {
    added: project.added,
    replaced: project.replaced,
    identical: project.identical,
    skipped: project.skipped,
  });
}

onMounted(() => transfer.startWatching(workspace.root));
onBeforeUnmount(() => transfer.stopWatching());
</script>

<template>
  <div class="stack">
    <p class="muted">{{ t("transfer.receiveIntro") }}</p>

    <section class="card stack">
      <div class="card-title">
        <h3>{{ t("transfer.found") }}</h3>
        <span class="row muted"><span class="spinner" /> {{ t("transfer.watching") }}</span>
      </div>

      <p v-if="!transfer.entries.length" class="muted">{{ t("transfer.nothing") }}</p>

      <div v-else class="list">
        <div v-for="entry in transfer.entries" :key="entry.path" class="list-row">
          <AppIcon name="folder" :size="16" />
          <span class="details">
            <span class="row">
              <span class="name">{{ entry.name }}</span>
              <span v-if="entry.packageName" class="badge badge-violet mono">{{ entry.packageName }}</span>
              <span v-if="!entry.isProject" class="badge badge-warn">{{ t("transfer.notProject") }}</span>
            </span>
            <span class="mono muted truncate selectable">{{ entry.path }}</span>
          </span>
          <label class="target">
            <span class="muted small">{{ t("transfer.target") }}</span>
            <select
              class="select"
              :value="transfer.targets[entry.path] ?? ''"
              @change="transfer.setTarget(entry.path, ($event.target as HTMLSelectElement).value)"
            >
              <option value="">{{ t("transfer.noTarget") }}</option>
              <option v-for="name in projectNames" :key="name" :value="name">{{ name }}</option>
            </select>
          </label>
        </div>
      </div>

      <div class="field">
        <label>{{ t("transfer.ignoreRun") }}</label>
        <textarea v-model="transfer.receivePatterns" class="textarea mono" rows="2" placeholder="*.local"></textarea>
      </div>

      <div class="toolbar">
        <ActionButton
          variant="primary"
          icon="search"
          :busy="transfer.receiveBusy"
          :disabled="!canAnalyze"
          @click="transfer.analyze(workspace.root)"
        >
          {{ t("transfer.analyze") }}
        </ActionButton>
      </div>
    </section>

    <NoticeBanner v-if="transfer.receiveError" tone="error" selectable>{{ transfer.receiveError }}</NoticeBanner>

    <section v-if="transfer.plan" class="card stack">
      <div class="card-title">
        <h3>{{ t("transfer.planTitle") }}</h3>
        <ActionButton
          variant="primary"
          small
          :busy="transfer.receiveBusy"
          :disabled="!canApply"
          @click="transfer.apply(workspace.root)"
        >
          {{ t("transfer.apply") }} ({{ transfer.selectedCount }})
        </ActionButton>
      </div>

      <div v-for="project in transfer.plan.projects" :key="project.source" class="project">
        <div class="toolbar">
          <strong>{{ project.target }}</strong>
          <span class="muted">{{ counts(project) }}</span>
          <span class="spacer" />
          <button class="chip" type="button" @click="transfer.selectFiles(project.source, 'changes')">
            {{ t("transfer.selectChanges") }}
          </button>
          <button class="chip" type="button" @click="transfer.selectFiles(project.source, 'all')">
            {{ t("common.all") }}
          </button>
          <button class="chip" type="button" @click="transfer.selectFiles(project.source, 'none')">
            {{ t("common.none") }}
          </button>
        </div>
        <div class="list files">
          <label
            v-for="file in project.files"
            :key="file.relative"
            class="list-row file"
            :class="{ selected: transfer.isSelected(project.source, file.relative) }"
          >
            <span class="check">
              <input
                type="checkbox"
                :checked="transfer.isSelected(project.source, file.relative)"
                @change="transfer.toggleFile(project.source, file.relative)"
              />
            </span>
            <span class="mono truncate path">{{ file.relative }}</span>
            <span class="badge" :class="statusClass(file.status)">{{ t(`transfer.statuses.${file.status}`) }}</span>
            <span class="mono muted size">{{ formatBytes(file.size) }}</span>
          </label>
        </div>
      </div>
    </section>

    <section v-if="transfer.receiveResult" class="card">
      <div class="card-title">
        <h3 class="ok">{{ t("transfer.receivedTitle") }}</h3>
        <span class="muted"
          >{{ formatDateTime(transfer.receiveResult.receivedAt) }} ·
          {{ formatBytes(transfer.receiveResult.bytes) }}</span
        >
      </div>
      <ul class="results">
        <li v-for="project in transfer.receiveResult.projects" :key="project.target">
          <strong>{{
            t("transfer.receivedInto", { added: project.added, replaced: project.replaced, target: project.target })
          }}</strong>
          <span class="mono muted small"> {{ project.files.join(", ") }}</span>
        </li>
      </ul>
      <p v-if="transfer.receiveResult.logFile" class="muted small selectable">
        {{ t("transfer.logWritten", { path: transfer.receiveResult.logFile }) }}
      </p>
    </section>
  </div>
</template>

<style scoped>
.list-row {
  flex-wrap: wrap;
}

.details {
  display: flex;
  flex-direction: column;
  min-width: 0;
  flex: 1 1 260px;
}

.name {
  font-weight: 600;
}

.target {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 220px;
}

.small {
  font-size: 12px;
}

.project {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.files {
  max-height: 320px;
  overflow: auto;
}

.file {
  cursor: pointer;
  padding: 6px 12px;
}

.path {
  flex: 1;
  font-size: 12.5px;
}

.size {
  font-size: 11.5px;
  min-width: 64px;
  text-align: right;
}

.results {
  margin: 0;
  padding-left: 18px;
  display: grid;
  gap: 6px;
}

.ok {
  color: var(--ok);
  text-transform: none;
  font-size: 15px;
  letter-spacing: 0;
}
</style>
