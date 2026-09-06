import { describe, expect, it } from "vitest";
import { Settings } from "luxon";

import { formatBytes, formatClock, formatDateTime, formatDuration } from "@/lib/format";

describe("formatDuration", () => {
  it("scales from tenths of seconds to hours", () => {
    expect(formatDuration(0)).toBe("0.0s");
    expect(formatDuration(900)).toBe("0.9s");
    expect(formatDuration(59_999)).toBe("59.9s");
    expect(formatDuration(65_000)).toBe("1m 05s");
    expect(formatDuration(3_723_000)).toBe("1h 02m 03s");
  });
});

describe("formatClock", () => {
  it("renders a stopwatch style clock", () => {
    expect(formatClock(5_000)).toBe("00:05");
    expect(formatClock(3_725_000)).toBe("1:02:05");
  });
});

describe("formatDateTime", () => {
  it("follows the active locale and keeps invalid input as is", () => {
    Settings.defaultLocale = "en-US";
    expect(formatDateTime("2026-09-06T13:05:00.000Z")).toMatch(/Sep 06, 2026, \d{2}:\d{2}/);
    expect(formatDateTime("not a date")).toBe("not a date");
  });
});

describe("formatBytes", () => {
  it("picks the unit and rounds small values to one decimal", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(20 * 1024 * 1024)).toBe("20 MB");
  });
});
