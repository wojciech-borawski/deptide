<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";

import type { StepName } from "@/api/types";
import PageHeader from "@/components/layout/PageHeader.vue";
import JobTable from "@/components/run/JobTable.vue";
import LogPane from "@/components/run/LogPane.vue";
import RunClock from "@/components/run/RunClock.vue";
import RunStatusLine from "@/components/run/RunStatusLine.vue";
import RerunDialog from "@/components/run/RerunDialog.vue";
import RunSummaryCard from "@/components/run/RunSummaryCard.vue";
import AppIcon from "@/components/ui/AppIcon.vue";
import EmptyState from "@/components/ui/EmptyState.vue";
import ProgressBar from "@/components/ui/ProgressBar.vue";
import NoticeBanner from "@/components/ui/NoticeBanner.vue";
import { describeError } from "@/composables/useAsyncAction";
import { buildBaseline } from "@/lib/timing-baseline";
import { routeNames } from "@/router";
import { useRunStore } from "@/stores/run";
import { useWizardStore } from "@/stores/wizard";
import { useWorkspaceStore } from "@/stores/workspace";

const run = useRunStore();
const wizard = useWizardStore();
const workspace = useWorkspaceStore();
const router = useRouter();
const { t } = useI18n();

const baseline = computed(() => buildBaseline(workspace.history, run.summary?.startedAt));
const rerunOpen = ref(false);
const rerunPreselected = ref<string[]>([]);
const notice = ref("");

const snapshot = computed(() => run.snapshot);
const nowMs = computed(() => (snapshot.value ? snapshot.value.startedAtMs + run.elapsedMs : 0));
const selectedLog = computed(() => (run.selectedJob ? run.logLinesFor(run.selectedJob) : []));
const failedNames = computed(() =>
  run.jobs.filter((job) => job.status === "failed" || job.status === "skipped").map((job) => job.name),
);
const restorable = computed(() =>
  run.isActive
    ? []
    : run.jobs
        .filter(
          (job) =>
            job.backupAvailable && (job.status === "failed" || job.status === "warn" || job.status === "skipped"),
        )
        .map((job) => job.name),
);
const phaseLabel = computed(() => {
  const current = snapshot.value?.currentPhase;
  if (!current || !snapshot.value) return null;
  return t("run.phase", {
    index: snapshot.value.steps.indexOf(current) + 1,
    total: snapshot.value.steps.length,
    step: t(`steps.${current}`),
  });
});

function newRun(): void {
  void router.push({ name: routeNames.wizard });
}

function retryFailed(): void {
  if (!snapshot.value) return;
  wizard.prefillRetry(snapshot.value);
  void router.push({ name: routeNames.wizard });
}

function openRerun(names: string[]): void {
  rerunPreselected.value = names;
  rerunOpen.value = true;
}

async function startRerun(projectNames: string[], steps: StepName[]): Promise<void> {
  const outcome = await run.rerun(workspace.root, projectNames, steps);
  if (outcome) rerunOpen.value = false;
}

async function restore(name: string): Promise<void> {
  try {
    const files = await run.restoreProject(name);
    notice.value = t("run.restored", { files: files.join(", "), name });
  } catch (cause) {
    notice.value = describeError(cause);
  }
}
</script>

<template>
  <div class="page">
    <template v-if="!snapshot">
      <PageHeader :title="t('run.title')" />
      <EmptyState icon="terminal" :title="t('run.noRun')" :text="t('run.noRunText')">
        <button class="btn btn-primary" type="button" @click="newRun">{{ t("run.openWizard") }}</button>
      </EmptyState>
    </template>

    <template v-else>
      <PageHeader :title="snapshot.label" :subtitle="snapshot.packages.join(', ')">
        <span v-if="phaseLabel" class="badge badge-violet">{{ phaseLabel }}</span>
        <span v-if="snapshot.dryRun" class="badge badge-warn">{{ t("common.dryRun") }}</span>
        <RunClock :elapsed-ms="run.elapsedMs" :live="run.isActive" />
        <button v-if="run.isActive" class="btn btn-danger" type="button" @click="run.abort">
          <AppIcon name="stop" :size="15" />
          {{ t("run.stopEverything") }}
        </button>
        <template v-else>
          <button v-if="run.lastPlan" class="btn" type="button" @click="openRerun(failedNames)">
            <AppIcon name="refresh" :size="15" />
            {{ t("run.rerunProjects") }}
          </button>
          <button
            v-if="failedNames.length"
            class="btn"
            type="button"
            :title="t('run.retryInWizardTitle')"
            @click="retryFailed"
          >
            {{ t("run.retryInWizard") }}
          </button>
          <button class="btn btn-primary" type="button" @click="newRun">
            <AppIcon name="play" :size="15" />
            {{ t("run.newRun") }}
          </button>
        </template>
      </PageHeader>

      <ProgressBar :percent="run.progressPercent" :done="!run.isActive" />

      <div class="body">
        <RunStatusLine
          :status-counts="run.statusCounts"
          :finished="run.finishedCount"
          :total="run.jobs.length"
          :parallel="snapshot.concurrency"
        />

        <NoticeBanner v-if="run.lastStartOutcome?.missing.length" tone="warn">
          {{ t("run.missing", { names: run.lastStartOutcome.missing.join(", ") }) }}
        </NoticeBanner>
        <NoticeBanner v-if="run.lastStartOutcome?.withoutPackages.length" tone="info">
          {{ t("run.withoutPackages", { names: run.lastStartOutcome.withoutPackages.join(", ") }) }}
        </NoticeBanner>
        <NoticeBanner v-if="run.error" tone="error" selectable>{{ run.error }}</NoticeBanner>
        <NoticeBanner v-if="notice" tone="info" selectable>{{ notice }}</NoticeBanner>

        <RunSummaryCard
          v-if="run.summary"
          :summary="run.summary"
          :baseline="baseline"
          :restorable="restorable"
          @restore="restore"
        />

        <div class="split">
          <JobTable
            :jobs="run.jobs"
            :steps="snapshot.steps"
            :selected="run.selectedJob"
            :now-ms="nowMs"
            :baseline="baseline"
            :rerunnable="!run.isActive && run.lastPlan !== null"
            :active="run.isActive"
            @select="run.selectJob"
            @rerun="openRerun([$event])"
            @stop="run.abortJob"
            @restore="restore"
          />
          <LogPane :title="run.selectedJob ?? t('run.title')" :lines="selectedLog" />
        </div>
      </div>

      <RerunDialog
        :open="rerunOpen"
        :jobs="run.jobs"
        :steps="snapshot.steps"
        :preselected="rerunPreselected"
        :busy="run.starting"
        @close="rerunOpen = false"
        @start="startRerun"
      />
    </template>
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
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 16px 24px;
  overflow: auto;
}

.split {
  flex: 1;
  min-height: 420px;
  display: grid;
  grid-template-rows: minmax(160px, 1fr) minmax(160px, 1fr);
  gap: 12px;
}
</style>
