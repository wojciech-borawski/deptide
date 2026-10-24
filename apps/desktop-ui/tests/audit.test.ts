import { describe, expect, it } from "vitest";

import type { AuditCounts } from "@/api/types";
import { auditBadges, auditTotals } from "@/lib/audit";

const billing: AuditCounts = { info: 0, low: 0, moderate: 1, high: 2, critical: 1, total: 4 };
const admin: AuditCounts = { info: 1, low: 1, moderate: 2, high: 0, critical: 0, total: 4 };
const clean: AuditCounts = { info: 0, low: 0, moderate: 0, high: 0, critical: 0, total: 0 };

describe("audit counts", () => {
  it("lists the severities that have findings, worst first", () => {
    expect(auditBadges(billing)).toEqual([
      { severity: "critical", count: 1 },
      { severity: "high", count: 2 },
      { severity: "moderate", count: 1 },
    ]);
    expect(auditBadges(clean)).toEqual([]);
  });

  it("adds up the projects that ran an audit report and ignores the rest", () => {
    expect(auditTotals([{ audit: billing }, {}, { audit: admin }, { audit: clean }])).toEqual({
      info: 1,
      low: 1,
      moderate: 3,
      high: 2,
      critical: 1,
      total: 8,
    });
    expect(auditTotals([{}, {}])).toBeNull();
  });
});
