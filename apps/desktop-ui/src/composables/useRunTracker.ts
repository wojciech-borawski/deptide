import { computed, reactive, ref } from "vue";
import { useIntervalFn } from "@vueuse/core";

import * as api from "@/api/commands";
import { listenRunEvents } from "@/api/events";
import type { JobSnapshot, JobStatus, RunEvent, RunSnapshot } from "@/api/types";

const maxLogLines = 2000;

const tickMs = 200;

export type StatusCounts = Record<JobStatus, number>;

export interface RunTrackerOptions {
  onFinished?: (snapshot: RunSnapshot) => void;
}

function withoutLogs(snapshot: RunSnapshot): RunSnapshot {
  return { ...snapshot, jobs: snapshot.jobs.map((job) => ({ ...job, log: [] })) };
}

export function useRunTracker(options: RunTrackerOptions = {}) {
  const snapshot = ref<RunSnapshot | null>(null);
  const logs = reactive<Record<string, string[]>>({});
  const elapsedMs = ref(0);
  const selectedJob = ref<string | null>(null);
  const error = ref("");
  const starting = ref(false);

  let startedAtMs = 0;
  let subscribed = false;
  let buffering = false;
  let pendingEvents: RunEvent[] = [];

  const clock = useIntervalFn(() => (elapsedMs.value = Date.now() - startedAtMs), tickMs, { immediate: false });

  const isActive = computed(() => snapshot.value !== null && snapshot.value.finishedAtMs === null);
  const jobs = computed(() => snapshot.value?.jobs ?? []);
  const summary = computed(() => snapshot.value?.summary ?? null);
  const statusCounts = computed<StatusCounts>(() => {
    const tally: StatusCounts = { ok: 0, warn: 0, failed: 0, skipped: 0, running: 0, pending: 0 };
    for (const job of jobs.value) tally[job.status] += 1;
    return tally;
  });
  const finishedCount = computed(
    () => statusCounts.value.ok + statusCounts.value.warn + statusCounts.value.failed + statusCounts.value.skipped,
  );
  const progressPercent = computed(() =>
    jobs.value.length ? Math.round((finishedCount.value / jobs.value.length) * 100) : 0,
  );

  function startClock(fromMs: number): void {
    startedAtMs = fromMs;
    elapsedMs.value = Date.now() - fromMs;
    clock.resume();
  }

  function replaceLogs(next: RunSnapshot): void {
    for (const key of Object.keys(logs)) delete logs[key];
    for (const job of next.jobs) logs[job.name] = [...job.log];
  }

  function replaceJobSnapshot(job: JobSnapshot): void {
    if (!snapshot.value) return;
    const index = snapshot.value.jobs.findIndex((entry) => entry.name === job.name);
    if (index >= 0) snapshot.value.jobs[index] = { ...job, log: [] };
  }

  function appendLogLine(job: string, line: string): void {
    const lines = logs[job] ?? (logs[job] = []);
    lines.push(line);
    if (lines.length > maxLogLines) lines.splice(0, lines.length - maxLogLines);
  }

  function handleEvent(event: RunEvent): void {
    if (buffering) {
      pendingEvents.push(event);
      return;
    }
    if (!snapshot.value || event.runId !== snapshot.value.id) return;

    switch (event.type) {
      case "jobChanged":
        replaceJobSnapshot(event.job);
        break;
      case "logLine":
        appendLogLine(event.job, event.line);
        break;
      case "phaseChanged":
        snapshot.value.currentPhase = event.phase;
        break;
      case "finished":
        snapshot.value = withoutLogs(event.snapshot);
        clock.pause();
        elapsedMs.value = event.snapshot.summary?.totalDurationMs ?? elapsedMs.value;
        options.onFinished?.(event.snapshot);
        break;
    }
  }

  async function subscribe(): Promise<void> {
    if (subscribed) return;
    subscribed = true;
    await listenRunEvents(handleEvent);
  }

  async function track<Outcome extends { snapshot: RunSnapshot }>(
    request: () => Promise<Outcome>,
  ): Promise<Outcome | null> {
    starting.value = true;
    error.value = "";
    buffering = true;
    pendingEvents = [];

    try {
      const outcome = await request();
      replaceLogs(outcome.snapshot);
      snapshot.value = withoutLogs(outcome.snapshot);
      selectedJob.value = outcome.snapshot.jobs[0]?.name ?? null;
      startClock(outcome.snapshot.startedAtMs);
      return outcome;
    } catch (cause) {
      error.value = String(cause);
      return null;
    } finally {
      starting.value = false;
      buffering = false;
      const replay = pendingEvents;
      pendingEvents = [];
      for (const event of replay) handleEvent(event);
    }
  }

  async function withCurrentRun(action: (runId: string) => Promise<void>): Promise<void> {
    if (!snapshot.value) return;
    try {
      await action(snapshot.value.id);
    } catch (cause) {
      error.value = String(cause);
    }
  }

  function abort(): Promise<void> {
    return withCurrentRun((runId) => api.abortRun(runId));
  }

  function abortJob(name: string): Promise<void> {
    return withCurrentRun((runId) => api.abortJob(runId, name));
  }

  function selectJob(name: string | null): void {
    selectedJob.value = name;
  }

  function logLinesFor(name: string): string[] {
    return logs[name] ?? [];
  }

  return {
    snapshot,
    logs,
    elapsedMs,
    selectedJob,
    error,
    starting,
    isActive,
    jobs,
    summary,
    statusCounts,
    finishedCount,
    progressPercent,
    subscribe,
    track,
    abort,
    abortJob,
    selectJob,
    logLinesFor,
  };
}
