import type { RunSummary } from "@/api/types";

export type RunOutcome = "finished" | "warnings" | "failures" | "stopped";

export type OutcomeTone = "ok" | "warn" | "failed" | "skipped";

type OutcomeSource = Pick<RunSummary, "aborted" | "failedCount" | "warnCount">;

export const outcomeTone: Record<RunOutcome, OutcomeTone> = {
  finished: "ok",
  warnings: "warn",
  failures: "failed",
  stopped: "skipped",
};

export const outcomeNoticeClass: Record<RunOutcome, string> = {
  finished: "notice-ok",
  warnings: "notice-warn",
  failures: "notice-error",
  stopped: "notice-warn",
};

export function classifyRunOutcome(summary: OutcomeSource): RunOutcome {
  if (summary.aborted) return "stopped";
  if (summary.failedCount > 0) return "failures";
  if (summary.warnCount > 0) return "warnings";
  return "finished";
}

export function outcomeCount(summary: OutcomeSource, outcome: RunOutcome): number {
  if (outcome === "failures") return summary.failedCount;
  if (outcome === "warnings") return summary.warnCount;
  return 0;
}
