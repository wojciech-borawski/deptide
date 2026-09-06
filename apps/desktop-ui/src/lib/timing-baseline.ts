import type { RunSummary, StepName, StepTiming } from "@/api/types";

export interface Baseline {
  medianMs: number;
  samples: number;
}

export interface Slowness {
  ratio: number;
  medianMs: number;
}

export type BaselineMap = Map<string, Baseline>;

const maxRuns = 12;

const minimumSamples = 2;

const minimumDurationMs = 10_000;

const slowRatio = 1.5;

function key(project: string, step: StepName): string {
  return `${project}|${step}`;
}

function median(values: number[]): number {
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  const lower = sorted[middle - 1] ?? sorted[middle] ?? 0;
  const upper = sorted[middle] ?? 0;
  return sorted.length % 2 === 0 ? (lower + upper) / 2 : upper;
}

export function buildBaseline(history: RunSummary[], excludeStartedAt?: string): BaselineMap {
  const samples = new Map<string, number[]>();

  const runs = history
    .filter((run) => !run.dryRun && !run.aborted && run.startedAt !== excludeStartedAt)
    .slice(0, maxRuns);

  for (const run of runs) {
    for (const project of run.projects) {
      if (project.status !== "ok" && project.status !== "warn") continue;
      for (const timing of project.stepTimings) {
        const list = samples.get(key(project.name, timing.step)) ?? [];
        list.push(timing.durationMs);
        samples.set(key(project.name, timing.step), list);
      }
    }
  }

  const baseline: BaselineMap = new Map();
  for (const [entry, values] of samples) {
    baseline.set(entry, { medianMs: median(values), samples: values.length });
  }
  return baseline;
}

export function slownessOf(project: string, timing: StepTiming, baseline: BaselineMap): Slowness | null {
  const known = baseline.get(key(project, timing.step));
  if (!known || known.samples < minimumSamples || known.medianMs <= 0) return null;
  if (timing.durationMs < minimumDurationMs) return null;

  const ratio = timing.durationMs / known.medianMs;
  return ratio >= slowRatio ? { ratio, medianMs: known.medianMs } : null;
}

export function slowestStep(
  project: string,
  timings: StepTiming[],
  baseline: BaselineMap,
): { step: StepName; slowness: Slowness } | null {
  let worst: { step: StepName; slowness: Slowness } | null = null;

  for (const timing of timings) {
    const slowness = slownessOf(project, timing, baseline);
    if (slowness && (!worst || slowness.ratio > worst.slowness.ratio)) {
      worst = { step: timing.step, slowness };
    }
  }

  return worst;
}
