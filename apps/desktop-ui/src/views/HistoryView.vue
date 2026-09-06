<script setup lang="ts">
import { computed, ref } from "vue";
import { openPath } from "@tauri-apps/plugin-opener";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";

import * as api from "@/api/commands";
import type { RunSummary, SavedRunFile } from "@/api/types";
import TrendChart, { type TrendPoint } from "@/components/history/TrendChart.vue";
import PageHeader from "@/components/layout/PageHeader.vue";
import AppIcon from "@/components/ui/AppIcon.vue";
import EmptyState from "@/components/ui/EmptyState.vue";
import ModalDialog from "@/components/ui/ModalDialog.vue";
import NoticeBanner from "@/components/ui/NoticeBanner.vue";
import { useCountedNoun } from "@/composables/useCountedNoun";
import { useAsyncAction } from "@/composables/useAsyncAction";
import { formatDateTime, formatDuration } from "@/lib/format";
import { classifyRunOutcome, outcomeCount, outcomeTone } from "@/lib/run-outcome";
import { saveReport } from "@/lib/report-export";
import { formatSpec } from "@/lib/versions";
import { routeNames } from "@/router";
import { useWizardStore } from "@/stores/wizard";
import { useWorkspaceStore } from "@/stores/workspace";

const workspace = useWorkspaceStore();
const wizard = useWizardStore();
const router = useRouter();
const { t } = useI18n();
const countOf = useCountedNoun();

const deleting = ref<SavedRunFile | null>(null);
const action = useAsyncAction();
const error = action.error;

const trendRuns = 20;

const realRuns = computed(() =>
  workspace.history
    .filter((run) => !run.dryRun)
    .slice(0, trendRuns)
    .reverse(),
);

const totalTimePoints = computed<TrendPoint[]>(() =>
  realRuns.value.map((run) => ({
    label: run.startedAt.slice(5, 10),
    value: run.totalDurationMs,
    tone: run.failedCount || run.aborted ? "failed" : run.warnCount ? "warn" : "ok",
  })),
);

const slowestProjects = computed<TrendPoint[]>(() => {
  const totals = new Map<string, { sum: number; count: number }>();

  for (const run of realRuns.value) {
    for (const project of run.projects) {
      if (project.status !== "ok" && project.status !== "warn") continue;
      const entry = totals.get(project.name) ?? { sum: 0, count: 0 };
      entry.sum += project.durationMs;
      entry.count += 1;
      totals.set(project.name, entry);
    }
  }

  return [...totals.entries()]
    .map(([name, entry]) => ({
      label: name.length > 14 ? `${name.slice(0, 13)}…` : name,
      value: entry.sum / entry.count,
    }))
    .sort((a, b) => b.value - a.value)
    .slice(0, 8);
});

const hasTrends = computed(() => realRuns.value.length >= 2);

function load(entry: SavedRunFile): void {
  wizard.prefillFromSavedRun(entry.run);
  void router.push({ name: routeNames.wizard });
}

async function confirmDelete(): Promise<void> {
  if (!deleting.value) return;

  const name = deleting.value.run.name;
  await action.run(async () => {
    await api.deleteSavedRun(workspace.root, name);
    await workspace.refresh();
  });
  deleting.value = null;
}

function tone(summary: RunSummary): string {
  return `badge-${outcomeTone[classifyRunOutcome(summary)]}`;
}

function outcome(summary: RunSummary): string {
  const kind = classifyRunOutcome(summary);
  return t(`history.outcome.${kind}`, { count: outcomeCount(summary, kind) });
}

function orderLabel(mode: RunSummary["mode"]): string {
  return mode === "per-step" ? t("common.stepByStep") : t("common.projectByProject");
}

async function openLog(summary: RunSummary): Promise<void> {
  if (summary.logFile) await openPath(summary.logFile);
}

async function exportReport(summary: RunSummary): Promise<void> {
  await action.run(() => saveReport(summary, "markdown"));
}
</script>

<template>
  <div class="page">
    <PageHeader :title="t('history.title')" :subtitle="workspace.root">
      <button class="btn btn-ghost btn-sm" type="button" @click="workspace.refresh">
        <AppIcon name="refresh" :size="14" />
        {{ t("common.refresh") }}
      </button>
    </PageHeader>

    <div class="body stack">
      <NoticeBanner v-if="error" tone="error" selectable>{{ error }}</NoticeBanner>

      <section>
        <h3 class="section-title">{{ t("history.trends") }}</h3>
        <p v-if="!hasTrends" class="muted">{{ t("history.notEnoughData") }}</p>
        <div v-else class="grid-2">
          <div class="card">
            <div class="card-title">
              <h3>{{ t("history.totalTimeTrend") }}</h3>
              <span class="muted">{{ t("history.lastRuns", { count: realRuns.length }) }}</span>
            </div>
            <TrendChart :points="totalTimePoints" />
          </div>
          <div class="card">
            <div class="card-title">
              <h3>{{ t("history.slowestProjects") }}</h3>
            </div>
            <TrendChart :points="slowestProjects" />
          </div>
        </div>
      </section>

      <section>
        <h3 class="section-title">{{ t("history.savedRuns") }}</h3>
        <EmptyState
          v-if="!workspace.savedRuns.length"
          icon="bolt"
          :title="t('history.noSaved')"
          :text="t('history.noSavedText')"
        />
        <div v-else class="list">
          <div v-for="entry in workspace.savedRuns" :key="entry.fileName" class="list-row saved">
            <span class="details">
              <span class="row">
                <span class="name">{{ entry.run.name }}</span>
                <span class="badge">{{ countOf("project", entry.run.projects.length) }}</span>
                <span class="badge badge-violet">{{
                  entry.run.steps.map((step) => t(`steps.${step}`)).join(" → ")
                }}</span>
                <span class="badge">{{ orderLabel(entry.run.mode) }} · {{ entry.run.concurrency }}×</span>
              </span>
              <span class="mono muted truncate">{{ entry.run.packages.map(formatSpec).join(", ") }}</span>
              <span class="muted small">{{ t("history.savedAt", { date: formatDateTime(entry.run.savedAt) }) }}</span>
            </span>
            <button class="btn btn-sm btn-primary" type="button" @click="load(entry)">
              <AppIcon name="play" :size="14" />
              {{ t("common.load") }}
            </button>
            <button
              class="btn btn-ghost btn-icon"
              type="button"
              :title="t('history.deleteTitle')"
              @click="deleting = entry"
            >
              <AppIcon name="trash" :size="15" />
            </button>
          </div>
        </div>
      </section>

      <section>
        <h3 class="section-title">{{ t("history.pastRuns") }}</h3>
        <EmptyState v-if="!workspace.history.length" icon="history" :title="t('history.noPast')" />
        <div v-else class="list">
          <div v-for="summary in workspace.history" :key="summary.startedAt" class="list-row saved">
            <span class="details">
              <span class="row">
                <span class="name">{{ summary.label }}</span>
                <span class="badge" :class="tone(summary)">{{ outcome(summary) }}</span>
                <span v-if="summary.dryRun" class="badge badge-warn">{{ t("common.dryRun") }}</span>
                <span class="badge">{{ countOf("project", summary.projectCount) }} · {{ summary.concurrency }}×</span>
              </span>
              <span class="mono muted truncate">{{ summary.packages.join(", ") }}</span>
              <span class="muted small">{{ formatDateTime(summary.startedAt) }}</span>
            </span>
            <span class="time">
              <span class="mono total">{{ formatDuration(summary.totalDurationMs) }}</span>
              <span class="muted small">{{
                t("run.summary.npmWork", { duration: formatDuration(summary.busyDurationMs) })
              }}</span>
            </span>
            <button
              class="btn btn-ghost btn-sm"
              type="button"
              :title="t('history.reportTitle')"
              @click="exportReport(summary)"
            >
              {{ t("common.report") }}
            </button>
            <button v-if="summary.logFile" class="btn btn-ghost btn-sm" type="button" @click="openLog(summary)">
              <AppIcon name="external" :size="14" />
              {{ t("common.log") }}
            </button>
          </div>
        </div>
      </section>
    </div>

    <ModalDialog :open="deleting !== null" :title="t('history.deleteTitle')" width="440px" @close="deleting = null">
      <p>{{ t("history.deleteText", { name: deleting?.run.name ?? "" }) }}</p>
      <template #footer>
        <button class="btn" type="button" @click="deleting = null">{{ t("common.cancel") }}</button>
        <button class="btn btn-danger" type="button" @click="confirmDelete">{{ t("common.delete") }}</button>
      </template>
    </ModalDialog>
  </div>
</template>

<style scoped>
.page {
  display: flex;
  flex-direction: column;
  height: 100%;
}

.body {
  flex: 1;
  overflow: auto;
  padding: 20px 24px;
  gap: 24px;
}

.section-title {
  margin-bottom: 10px;
}

.details {
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-width: 0;
  flex: 1;
}

.name {
  font-weight: 600;
}

.small {
  font-size: 12px;
}

.time {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  min-width: 120px;
}

.total {
  font-weight: 700;
  color: var(--accent);
}
</style>
