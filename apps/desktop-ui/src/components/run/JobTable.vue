<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import type { JobSnapshot, StepName } from "@/api/types";
import AppIcon from "@/components/ui/AppIcon.vue";
import StatusBadge from "@/components/ui/StatusBadge.vue";
import { useCountedNoun } from "@/composables/useCountedNoun";
import { localizeDiagnosis, shortHash } from "@/lib/diagnosis";
import { formatDuration } from "@/lib/format";
import { slowestStep, type BaselineMap } from "@/lib/timing-baseline";

const props = defineProps<{
  jobs: JobSnapshot[];
  steps: StepName[];
  selected: string | null;
  nowMs: number;
  baseline: BaselineMap;
  rerunnable: boolean;
  active: boolean;
}>();
const emit = defineEmits<{
  select: [name: string];
  rerun: [name: string];
  stop: [name: string];
  restore: [name: string];
}>();
const { t, te } = useI18n();
const countOf = useCountedNoun();

const stepColors: Record<StepName, string> = {
  uninstall: "var(--violet)",
  install: "var(--accent)",
  "force-install": "var(--ok)",
  audit: "var(--warn)",
  build: "var(--running)",
};

const longest = computed(() => Math.max(1, ...props.jobs.map((job) => liveDuration(job))));

function liveDuration(job: JobSnapshot): number {
  if (job.status === "running" && job.stepStartedAtMs !== null) {
    return job.durationMs + Math.max(0, props.nowMs - job.stepStartedAtMs);
  }
  return job.durationMs;
}

function segmentWidth(durationMs: number): string {
  return `${(durationMs / longest.value) * 100}%`;
}

function runningWidth(job: JobSnapshot): string {
  if (job.status !== "running" || job.stepStartedAtMs === null) return "0%";
  const current = Math.max(0, props.nowMs - job.stepStartedAtMs);
  return `${(current / longest.value) * 100}%`;
}

function detail(job: JobSnapshot): string {
  if (job.status === "failed") return job.diagnosis ? localizeDiagnosis(job.diagnosis, t, te).title : job.error;
  if (job.status === "warn") return job.warning;
  if (job.status === "running") return job.currentStep;
  if (job.status === "skipped") return job.error || t("run.stoppedBeforeFinished");
  if (job.status === "pending" && job.dependsOn.length) return t("run.waitsFor", { names: job.dependsOn.join(", ") });
  if (job.status === "ok" && job.installed.length) {
    return job.installed
      .map((entry) => `${entry.installed ?? "?"}${entry.integrity ? ` · ${shortHash(entry.integrity)}` : ""}`)
      .join(", ");
  }
  return job.currentStep;
}

function detailTitle(job: JobSnapshot): string {
  if (job.status === "failed") {
    return job.diagnosis ? `${job.error}\n${localizeDiagnosis(job.diagnosis, t, te).hint}` : job.error;
  }
  if (job.status === "ok" && job.installed.length) {
    return job.installed
      .map((entry) => `${entry.name}@${entry.installed ?? "missing"}${entry.integrity ? `\n${entry.integrity}` : ""}`)
      .join("\n");
  }
  return detail(job);
}

function slowBadge(job: JobSnapshot): string | null {
  if (job.status !== "ok" && job.status !== "warn") return null;
  const slow = slowestStep(job.name, job.stepTimings, props.baseline);
  return slow ? t("run.slower", { step: t(`steps.${slow.step}`), ratio: slow.slowness.ratio.toFixed(1) }) : null;
}

function canStop(job: JobSnapshot): boolean {
  return props.active && (job.status === "running" || job.status === "pending");
}

function canRestore(job: JobSnapshot): boolean {
  return (
    !props.active &&
    job.backupAvailable &&
    (job.status === "failed" || job.status === "warn" || job.status === "skipped")
  );
}
</script>

<template>
  <div class="table">
    <div class="head">
      <span>{{ t("run.columns.status") }}</span>
      <span>{{ t("run.columns.project") }}</span>
      <span>{{ t("run.columns.detail") }}</span>
      <span>{{ t("run.columns.timeline") }}</span>
      <span class="right">{{ t("run.columns.time") }}</span>
    </div>
    <button
      v-for="job in props.jobs"
      :key="job.name"
      type="button"
      class="job"
      :class="{ selected: job.name === props.selected, [job.status]: true }"
      @click="emit('select', job.name)"
    >
      <span><StatusBadge :status="job.status" /></span>
      <span class="name">
        <span class="truncate">{{ job.name }}</span>
        <span class="mono muted truncate packages" :title="job.packages.join(', ')">{{ job.packages.join(", ") }}</span>
      </span>
      <span class="detail">
        <span
          class="truncate"
          :class="{
            error: job.status === 'failed',
            warning: job.status === 'warn',
            mono: job.status === 'ok' && job.installed.length > 0,
          }"
          :title="detailTitle(job)"
        >
          {{ detail(job) }}
        </span>
        <span class="row wrap">
          <span v-if="slowBadge(job)" class="badge badge-warn small">{{ slowBadge(job) }}</span>
          <span v-if="job.retries" class="badge small">{{ t("run.retried", { count: job.retries }) }}</span>
          <span v-if="job.dependencyChanges.length" class="badge badge-violet small">
            {{
              t("run.changes", {
                count: countOf("dependency", job.dependencyChanges.length),
              })
            }}
          </span>
        </span>
      </span>
      <span
        class="timeline"
        :title="
          job.stepTimings.map((entry) => `${t(`steps.${entry.step}`)} ${formatDuration(entry.durationMs)}`).join(' · ')
        "
      >
        <span
          v-for="(timing, index) in job.stepTimings"
          :key="index"
          class="segment"
          :style="{ width: segmentWidth(timing.durationMs), background: stepColors[timing.step] }"
        />
        <span v-if="job.status === 'running'" class="segment live" :style="{ width: runningWidth(job) }" />
      </span>
      <span class="right mono duration">
        {{ liveDuration(job) ? formatDuration(liveDuration(job)) : "-" }}
        <span
          v-if="canStop(job)"
          class="btn btn-ghost btn-icon action"
          role="button"
          :title="t('run.stopProject', { name: job.name })"
          @click.stop="emit('stop', job.name)"
        >
          <AppIcon name="stop" :size="14" />
        </span>
        <span
          v-if="canRestore(job)"
          class="btn btn-ghost btn-icon action"
          role="button"
          :title="t('run.restoreTitle')"
          @click.stop="emit('restore', job.name)"
        >
          <AppIcon name="history" :size="14" />
        </span>
        <span
          v-if="props.rerunnable"
          class="btn btn-ghost btn-icon action"
          role="button"
          :title="t('run.rerunProject', { name: job.name })"
          @click.stop="emit('rerun', job.name)"
        >
          <AppIcon name="refresh" :size="14" />
        </span>
      </span>
    </button>
  </div>
</template>

<style scoped>
.table {
  display: flex;
  flex-direction: column;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--bg-panel);
  overflow: auto;
  container-type: inline-size;
}

.head,
.job {
  display: grid;
  grid-template-columns: 110px minmax(160px, 1.2fr) minmax(160px, 1.5fr) minmax(140px, 1fr) 130px;
  gap: 12px;
  align-items: center;
  padding: 8px 14px;
}

.head {
  position: sticky;
  top: 0;
  background: var(--bg-elevated);
  border-bottom: 1px solid var(--border);
  font-size: 11.5px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--text-muted);
  z-index: 1;
}

.job {
  border: none;
  border-bottom: 1px solid var(--border);
  background: transparent;
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: pointer;
  min-height: 48px;
  transition: background 0.1s ease;
}

.job:last-child {
  border-bottom: none;
}

.job:hover {
  background: var(--bg-hover);
}

.job.selected {
  background: var(--accent-soft);
  box-shadow: inset 3px 0 0 var(--accent);
}

.job.failed.selected {
  box-shadow: inset 3px 0 0 var(--failed);
}

.name {
  display: flex;
  flex-direction: column;
  min-width: 0;
  font-weight: 600;
}

.packages {
  font-weight: 400;
  font-size: 11.5px;
}

.detail {
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-width: 0;
  color: var(--text-muted);
  font-size: 12.5px;
}

.detail .error {
  color: var(--failed);
}

.detail .warning {
  color: var(--warn);
}

.wrap {
  flex-wrap: wrap;
  gap: 4px;
}

.small {
  font-size: 10.5px;
}

@container (max-width: 760px) {
  .head,
  .job {
    grid-template-columns: 100px minmax(140px, 1fr) minmax(140px, 1.5fr) 110px;
  }

  .head span:nth-child(4),
  .timeline {
    display: none;
  }
}

.timeline {
  display: flex;
  height: 8px;
  border-radius: 4px;
  background: var(--bg-input);
  overflow: hidden;
}

.segment {
  height: 100%;
  min-width: 2px;
}

.segment.live {
  background: repeating-linear-gradient(90deg, var(--running) 0 6px, rgba(56, 189, 248, 0.45) 6px 12px);
  animation: slide 0.8s linear infinite;
}

@keyframes slide {
  to {
    background-position: 12px 0;
  }
}

.right {
  text-align: right;
}

.duration {
  color: var(--text-muted);
  display: inline-flex;
  align-items: center;
  justify-content: flex-end;
  gap: 4px;
}

.action {
  opacity: 0;
  padding: 3px;
  transition: opacity 0.1s ease;
}

.job:hover .action,
.job.selected .action {
  opacity: 1;
}
</style>
