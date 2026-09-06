<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import PageHeader from "@/components/layout/PageHeader.vue";
import LogPane from "@/components/run/LogPane.vue";
import RunClock from "@/components/run/RunClock.vue";
import RunStatusLine from "@/components/run/RunStatusLine.vue";
import CommandPrompt from "@/components/terminal/CommandPrompt.vue";
import CommandTable from "@/components/terminal/CommandTable.vue";
import TerminalProjectPicker from "@/components/terminal/TerminalProjectPicker.vue";
import AppIcon from "@/components/ui/AppIcon.vue";
import NoticeBanner from "@/components/ui/NoticeBanner.vue";
import ProgressBar from "@/components/ui/ProgressBar.vue";
import { formatDuration } from "@/lib/format";
import { classifyRunOutcome, outcomeNoticeClass } from "@/lib/run-outcome";
import { useTerminalStore } from "@/stores/terminal";
import { useWorkspaceStore } from "@/stores/workspace";

const workspace = useWorkspaceStore();
const terminal = useTerminalStore();
const { t } = useI18n();

const snapshot = computed(() => terminal.snapshot);
const nowMs = computed(() => (snapshot.value ? snapshot.value.startedAtMs + terminal.elapsedMs : 0));
const selectedLog = computed(() => (terminal.selectedJob ? terminal.logLinesFor(terminal.selectedJob) : []));
const outcome = computed(() => (terminal.summary ? classifyRunOutcome(terminal.summary) : null));

function lastLine(name: string): string {
  const lines = terminal.logLinesFor(name);
  for (let index = lines.length - 1; index >= 0; index -= 1) {
    const line = lines[index]?.trim() ?? "";
    if (line) return line;
  }
  return "";
}
</script>

<template>
  <div class="page">
    <PageHeader :title="t('terminal.title')" :subtitle="workspace.root">
      <RunClock v-if="snapshot" :elapsed-ms="terminal.elapsedMs" :live="terminal.isActive" />
      <button v-if="terminal.isActive" class="btn btn-danger" type="button" @click="terminal.abort">
        <AppIcon name="stop" :size="15" />
        {{ t("run.stopEverything") }}
      </button>
    </PageHeader>

    <ProgressBar :percent="terminal.progressPercent" :done="!terminal.isActive" />

    <div class="body">
      <div class="layout">
        <TerminalProjectPicker />

        <div class="main">
          <CommandPrompt />

          <NoticeBanner v-if="terminal.error" tone="error" selectable>{{ terminal.error }}</NoticeBanner>

          <template v-if="snapshot">
            <RunStatusLine
              :status-counts="terminal.statusCounts"
              :finished="terminal.finishedCount"
              :total="terminal.jobs.length"
              :parallel="snapshot.concurrency"
            >
              <span class="mono command-label truncate" :title="snapshot.command ?? ''">$ {{ snapshot.command }}</span>
            </RunStatusLine>

            <p v-if="outcome && terminal.summary" class="notice" :class="outcomeNoticeClass[outcome]">
              {{
                t(`terminal.outcome.${outcome}`, {
                  time: formatDuration(terminal.summary.totalDurationMs),
                  busy: formatDuration(terminal.summary.busyDurationMs),
                })
              }}
            </p>

            <div class="split">
              <CommandTable
                :jobs="terminal.jobs"
                :selected="terminal.selectedJob"
                :now-ms="nowMs"
                :active="terminal.isActive"
                :last-line="lastLine"
                @select="terminal.selectJob"
                @stop="terminal.abortJob"
              />
              <LogPane :title="terminal.selectedJob ?? t('terminal.title')" :lines="selectedLog" />
            </div>
          </template>

          <div v-else class="idle card">
            <AppIcon name="terminal" :size="28" />
            <p class="muted">{{ t("terminal.intro") }}</p>
          </div>
        </div>
      </div>
    </div>
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
  padding: 16px 24px;
  overflow: auto;
  container-type: inline-size;
}

.layout {
  display: grid;
  grid-template-columns: minmax(240px, 300px) minmax(0, 1fr);
  gap: 16px;
  min-height: 100%;
  align-items: start;
}

.main {
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-width: 0;
}

.command-label {
  color: var(--accent);
  font-weight: 600;
  max-width: 50%;
}

.split {
  min-height: 480px;
  height: calc(100vh - 330px);
  display: grid;
  grid-template-rows: minmax(160px, 1fr) minmax(160px, 1fr);
  gap: 12px;
  min-width: 0;
}

@container (max-width: 760px) {
  .layout {
    grid-template-columns: minmax(0, 1fr);
  }

  .command-label {
    max-width: 100%;
  }
}

.idle {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  padding: 48px 24px;
  text-align: center;
  color: var(--text-muted);
}
</style>
