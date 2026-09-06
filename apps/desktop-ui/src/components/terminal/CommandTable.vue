<script setup lang="ts">
import { useI18n } from "vue-i18n";

import type { JobSnapshot } from "@/api/types";
import AppIcon from "@/components/ui/AppIcon.vue";
import StatusBadge from "@/components/ui/StatusBadge.vue";
import { formatDuration } from "@/lib/format";

const props = defineProps<{
  jobs: JobSnapshot[];
  selected: string | null;
  nowMs: number;
  active: boolean;
  lastLine: (name: string) => string;
}>();
const emit = defineEmits<{ select: [name: string]; stop: [name: string] }>();
const { t } = useI18n();

function liveDuration(job: JobSnapshot): number {
  if (job.status === "running" && job.startedAtMs !== null) {
    return Math.max(0, props.nowMs - job.startedAtMs);
  }
  return job.durationMs;
}

function detail(job: JobSnapshot): string {
  if (job.status === "failed") return job.error;
  if (job.status === "skipped") return job.error || t("run.stoppedBeforeFinished");
  if (job.status === "pending") return t("terminal.waiting");
  return props.lastLine(job.name);
}

function canStop(job: JobSnapshot): boolean {
  return props.active && (job.status === "running" || job.status === "pending");
}
</script>

<template>
  <div class="table">
    <div class="head">
      <span>{{ t("run.columns.status") }}</span>
      <span>{{ t("run.columns.project") }}</span>
      <span>{{ t("terminal.columns.output") }}</span>
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
        <span class="mono muted truncate path" :title="job.directory">{{ job.directory }}</span>
      </span>
      <span
        class="detail mono truncate"
        :class="{ error: job.status === 'failed', faint: job.status === 'pending' }"
        :title="detail(job)"
      >
        {{ detail(job) }}
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
  grid-template-columns: 100px minmax(140px, 1fr) minmax(160px, 2fr) 96px;
  gap: 12px;
  align-items: center;
  padding: 8px 14px;
}

@container (max-width: 560px) {
  .head,
  .job {
    grid-template-columns: 100px minmax(0, 1fr) 96px;
  }

  .head span:nth-child(3),
  .detail {
    display: none;
  }
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

.path {
  font-weight: 400;
  font-size: 11.5px;
}

.detail {
  min-width: 0;
  color: var(--text-muted);
  font-size: 12px;
}

.detail.error {
  color: var(--failed);
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
