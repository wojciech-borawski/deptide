import { describe, expect, it } from "vitest";

import type { RunProjectSummary, RunSummary } from "@/api/types";
import { buildBaseline, slowestStep, slownessOf } from "@/lib/timing-baseline";

function project(name: string, buildMs: number, status: RunProjectSummary["status"] = "ok"): RunProjectSummary {
  return {
    name,
    directory: `C:/repos/${name}`,
    status,
    durationMs: buildMs + 5_000,
    stepTimings: [
      { step: "install", durationMs: 5_000 },
      { step: "build", durationMs: buildMs },
    ],
    error: "",
    warning: "",
    installed: [],
    diagnosis: null,
    dependencyChanges: [],
    retries: 0,
  };
}

function run(startedAt: string, projects: RunProjectSummary[], overrides: Partial<RunSummary> = {}): RunSummary {
  return {
    label: "run",
    packages: [],
    projectCount: projects.length,
    concurrency: 3,
    steps: ["install", "build"],
    mode: "per-project",
    dryRun: false,
    startedAt,
    finishedAt: startedAt,
    totalDurationMs: 60_000,
    busyDurationMs: 60_000,
    logFile: null,
    projects,
    okCount: projects.length,
    warnCount: 0,
    failedCount: 0,
    skippedCount: 0,
    aborted: false,
    ...overrides,
  };
}

const history: RunSummary[] = [
  run("2026-09-06T10:00:00Z", [project("web", 90_000)]),
  run("2026-09-05T10:00:00Z", [project("web", 30_000)]),
  run("2026-09-04T10:00:00Z", [project("web", 32_000)]),
  run("2026-09-03T10:00:00Z", [project("web", 28_000)]),
  run("2026-09-02T10:00:00Z", [project("web", 500_000, "failed")]),
  run("2026-09-01T10:00:00Z", [project("web", 600_000)], { dryRun: true }),
];

describe("buildBaseline", () => {
  it("uses the median of successful real runs and can exclude the current one", () => {
    const baseline = buildBaseline(history, "2026-09-06T10:00:00Z");
    expect(baseline.get("web|build")).toEqual({ medianMs: 30_000, samples: 3 });
    expect(baseline.get("web|install")?.samples).toBe(3);
  });

  it("ignores failed projects and dry runs", () => {
    const baseline = buildBaseline(history);
    expect(baseline.get("web|build")?.samples).toBe(4);
  });
});

describe("slownessOf", () => {
  const baseline = buildBaseline(history, "2026-09-06T10:00:00Z");

  it("flags a step well above its median", () => {
    const slow = slownessOf("web", { step: "build", durationMs: 90_000 }, baseline);
    expect(slow?.ratio).toBeCloseTo(3);
    expect(slow?.medianMs).toBe(30_000);
  });

  it("stays quiet for normal, short or unknown steps", () => {
    expect(slownessOf("web", { step: "build", durationMs: 35_000 }, baseline)).toBeNull();
    expect(slownessOf("web", { step: "install", durationMs: 9_000 }, baseline)).toBeNull();
    expect(slownessOf("api", { step: "build", durationMs: 90_000 }, baseline)).toBeNull();
  });

  it("returns the worst step of a project", () => {
    const worst = slowestStep(
      "web",
      [
        { step: "install", durationMs: 5_000 },
        { step: "build", durationMs: 75_000 },
      ],
      baseline,
    );
    expect(worst?.step).toBe("build");
    expect(worst?.slowness.ratio).toBeCloseTo(2.5);
  });
});
