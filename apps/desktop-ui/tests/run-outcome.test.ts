import { describe, expect, it } from "vitest";

import { classifyRunOutcome, outcomeCount, outcomeTone } from "@/lib/run-outcome";

describe("classifyRunOutcome", () => {
  it("ranks stopped above failures above warnings", () => {
    expect(classifyRunOutcome({ aborted: true, failedCount: 2, warnCount: 1 })).toBe("stopped");
    expect(classifyRunOutcome({ aborted: false, failedCount: 2, warnCount: 1 })).toBe("failures");
    expect(classifyRunOutcome({ aborted: false, failedCount: 0, warnCount: 1 })).toBe("warnings");
    expect(classifyRunOutcome({ aborted: false, failedCount: 0, warnCount: 0 })).toBe("finished");
  });

  it("maps every outcome to a tone and a count", () => {
    const summary = { aborted: false, failedCount: 3, warnCount: 2 };
    expect(outcomeTone.failures).toBe("failed");
    expect(outcomeTone.stopped).toBe("skipped");
    expect(outcomeCount(summary, "failures")).toBe(3);
    expect(outcomeCount(summary, "warnings")).toBe(2);
    expect(outcomeCount(summary, "finished")).toBe(0);
  });
});
