<script setup lang="ts">
import { computed, ref } from "vue";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import { useI18n } from "vue-i18n";

import type { RunProjectSummary, RunSummary } from "@/api/types";
import AppIcon from "@/components/ui/AppIcon.vue";
import StatusBadge from "@/components/ui/StatusBadge.vue";
import { describeError } from "@/composables/useAsyncAction";
import { localizeDiagnosis, shortHash } from "@/lib/diagnosis";
import { formatDateTime, formatDuration } from "@/lib/format";
import { copyReport, saveReport } from "@/lib/report-export";
import { classifyRunOutcome, outcomeTone } from "@/lib/run-outcome";
import { slowestStep, type BaselineMap } from "@/lib/timing-baseline";
import { useToastStore } from "@/stores/toasts";

const props = defineProps<{ summary: RunSummary; baseline: BaselineMap; restorable: string[] }>();
const emit = defineEmits<{ restore: [name: string] }>();
const { t, te } = useI18n();

const toasts = useToastStore();
const expanded = ref<Set<string>>(new Set());

const hasRestorable = computed(() => props.restorable.length > 0);

const speedup = computed(() => {
  if (!props.summary.totalDurationMs) return null;
  return (props.summary.busyDurationMs / props.summary.totalDurationMs).toFixed(1);
});

const headline = computed(() => {
  const outcome = classifyRunOutcome(props.summary);
  return { text: t(`run.summary.${outcome}`), tone: outcomeTone[outcome] };
});

function slowness(project: RunProjectSummary): string | null {
  const slow = slowestStep(project.name, project.stepTimings, props.baseline);
  if (!slow) return null;
  return t("run.slowerWithMedian", {
    step: t(`steps.${slow.step}`),
    ratio: slow.slowness.ratio.toFixed(1),
    median: formatDuration(slow.slowness.medianMs),
  });
}

function diagnosis(project: RunProjectSummary): { title: string; hint: string } | null {
  return project.diagnosis ? localizeDiagnosis(project.diagnosis, t, te) : null;
}

function toggleChanges(name: string): void {
  const next = new Set(expanded.value);
  if (next.has(name)) next.delete(name);
  else next.add(name);
  expanded.value = next;
}

async function openLog(): Promise<void> {
  if (props.summary.logFile) await openPath(props.summary.logFile);
}

async function revealLog(): Promise<void> {
  if (props.summary.logFile) await revealItemInDir(props.summary.logFile);
}

async function copyMarkdown(): Promise<void> {
  try {
    await copyReport(props.summary, "markdown");
    toasts.push({ tone: "ok", text: t("run.summary.copied") });
  } catch (cause) {
    toasts.push({ tone: "error", text: describeError(cause) });
  }
}

async function saveAs(format: "markdown" | "html"): Promise<void> {
  try {
    const path = await saveReport(props.summary, format);
    if (path) toasts.push({ tone: "ok", text: t("run.summary.saved", { path }) });
  } catch (cause) {
    toasts.push({ tone: "error", text: describeError(cause) });
  }
}
</script>

<template>
  <section class="card summary">
    <div class="hero">
      <div>
        <h2 :class="`tone-${headline.tone}`">{{ headline.text }}</h2>
        <p class="muted">
          {{ formatDateTime(props.summary.startedAt) }} → {{ formatDateTime(props.summary.finishedAt) }}
          <span v-if="props.summary.dryRun" class="badge badge-warn">{{ t("common.dryRun") }}</span>
        </p>
      </div>
      <div class="total">
        <span class="label">{{ t("run.summary.totalTime") }}</span>
        <span class="value mono">{{ formatDuration(props.summary.totalDurationMs) }}</span>
        <span class="muted">
          {{ t("run.summary.npmWork", { duration: formatDuration(props.summary.busyDurationMs) }) }}
          <template v-if="speedup"> · {{ t("run.summary.parallel", { factor: speedup }) }}</template>
        </span>
      </div>
    </div>

    <div class="counts">
      <span class="badge badge-ok">{{ t("run.summary.ok", { count: props.summary.okCount }) }}</span>
      <span class="badge badge-warn">{{ t("run.summary.warn", { count: props.summary.warnCount }) }}</span>
      <span class="badge badge-failed">{{ t("run.summary.failed", { count: props.summary.failedCount }) }}</span>
      <span class="badge badge-skipped">{{ t("run.summary.skipped", { count: props.summary.skippedCount }) }}</span>
      <span class="spacer" />
      <button class="btn btn-sm" type="button" @click="copyMarkdown">{{ t("run.summary.copyMarkdown") }}</button>
      <button class="btn btn-sm" type="button" @click="saveAs('markdown')">{{ t("run.summary.saveMd") }}</button>
      <button class="btn btn-sm" type="button" @click="saveAs('html')">{{ t("run.summary.saveHtml") }}</button>
      <button v-if="props.summary.logFile" class="btn btn-sm" type="button" @click="openLog">
        <AppIcon name="external" :size="14" />
        {{ t("common.openLog") }}
      </button>
      <button v-if="props.summary.logFile" class="btn btn-ghost btn-sm" type="button" @click="revealLog">
        {{ t("common.showInFolder") }}
      </button>
    </div>
    <table class="projects selectable">
      <thead>
        <tr>
          <th>{{ t("run.columns.project") }}</th>
          <th>{{ t("run.columns.status") }}</th>
          <th>{{ t("run.columns.installed") }}</th>
          <th>{{ t("run.columns.steps") }}</th>
          <th class="right">{{ t("run.columns.time") }}</th>
          <th v-if="hasRestorable" class="right">{{ t("run.columns.actions") }}</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="project in props.summary.projects" :key="project.name">
          <td>
            <div class="row">
              <span>{{ project.name }}</span>
              <span v-if="project.retries" class="badge small">{{ t("run.retried", { count: project.retries }) }}</span>
            </div>
            <div v-if="project.error || project.warning" class="detail" :class="project.error ? 'error' : 'warning'">
              {{ project.error || project.warning }}
            </div>
            <div v-if="diagnosis(project)" class="detail hint">
              <strong>{{ diagnosis(project)?.title }}.</strong> {{ diagnosis(project)?.hint }}
            </div>
            <div v-if="slowness(project)" class="detail warning">{{ slowness(project) }}</div>
            <div v-if="project.dependencyChanges.length" class="detail">
              <button class="link" type="button" @click="toggleChanges(project.name)">
                {{
                  t("run.changes", {
                    count: t(
                      "common.dependency",
                      { count: project.dependencyChanges.length },
                      project.dependencyChanges.length,
                    ),
                  })
                }}
                · {{ expanded.has(project.name) ? t("run.hideChanges") : t("run.showChanges") }}
              </button>
              <ul v-if="expanded.has(project.name)" class="changes mono">
                <li v-for="change in project.dependencyChanges" :key="change.name">
                  {{ change.name }}
                  <span class="faint">{{ change.before ?? t("run.added") }}</span>
                  →
                  <span :class="change.after ? '' : 'warning'">{{ change.after ?? t("run.removed") }}</span>
                </li>
              </ul>
            </div>
          </td>
          <td><StatusBadge :status="project.status" /></td>
          <td class="mono installed">
            <div
              v-for="entry in project.installed"
              :key="entry.name"
              :class="{ mismatch: !entry.matches }"
              :title="entry.integrity ?? ''"
            >
              {{ entry.name }}@{{ entry.installed ?? "-" }}
              <span v-if="entry.integrity" class="faint"> {{ shortHash(entry.integrity) }}</span>
              <span v-if="!entry.matches" class="warning">
                {{ t("run.summary.expected", { version: entry.expected }) }}</span
              >
            </div>
            <span v-if="!project.installed.length" class="faint">-</span>
          </td>
          <td class="mono muted">
            <span class="steps">
              <span v-for="(timing, index) in project.stepTimings" :key="index">
                {{ t(`steps.${timing.step}`) }} {{ formatDuration(timing.durationMs) }}
              </span>
            </span>
          </td>
          <td class="right mono">{{ formatDuration(project.durationMs) }}</td>
          <td v-if="hasRestorable" class="right actions">
            <button
              v-if="props.restorable.includes(project.name)"
              v-ripple
              class="btn btn-ghost btn-sm"
              type="button"
              :title="t('run.restoreTitle')"
              @click="emit('restore', project.name)"
            >
              <AppIcon name="history" :size="13" />
              {{ t("run.restore") }}
            </button>
          </td>
        </tr>
      </tbody>
    </table>
  </section>
</template>

<style scoped>
.summary {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.hero {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 16px;
}

.hero h2 {
  font-size: 19px;
}

.tone-ok {
  color: var(--ok);
}

.tone-warn {
  color: var(--warn);
}

.tone-skipped {
  color: var(--skipped, var(--text-muted));
}

.tone-failed {
  color: var(--failed);
}

.total {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  text-align: right;
  padding: 10px 16px;
  border-radius: var(--radius);
  background: var(--bg-elevated);
  border: 1px solid var(--border);
}

.total .label {
  font-size: 11.5px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--text-muted);
}

.total .value {
  font-size: 26px;
  font-weight: 700;
  background: var(--gradient);
  -webkit-background-clip: text;
  background-clip: text;
  color: transparent;
}

.counts {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.projects {
  width: 100%;
  border-collapse: collapse;
  font-size: 13px;
}

.projects th {
  text-align: left;
  font-size: 11.5px;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--text-muted);
  padding: 6px 8px;
  border-bottom: 1px solid var(--border);
}

.projects td {
  padding: 8px;
  border-bottom: 1px solid var(--border);
  vertical-align: top;
}

.projects tr:last-child td {
  border-bottom: none;
}

.right {
  text-align: right;
}

.detail {
  font-size: 12px;
}

.detail.error {
  color: var(--failed);
}

.detail.warning,
.warning {
  color: var(--warn);
}

.detail.hint {
  color: var(--text-muted);
  margin-top: 2px;
}

.small {
  font-size: 10.5px;
}

.link {
  border: none;
  background: transparent;
  color: var(--accent);
  padding: 0;
  font: inherit;
  cursor: pointer;
}

.changes {
  margin: 4px 0 0;
  padding-left: 16px;
  font-size: 11.5px;
  color: var(--text-muted);
}

.installed {
  font-size: 11.5px;
}

.installed .mismatch {
  color: var(--warn);
}

.steps {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 12px;
  font-size: 11.5px;
}

.actions {
  white-space: nowrap;
}
</style>
