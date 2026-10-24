import type { AuditCounts } from "@/api/types";

export type AuditSeverity = "critical" | "high" | "moderate" | "low" | "info";

export const auditSeverities: readonly AuditSeverity[] = ["critical", "high", "moderate", "low", "info"];

export function auditBadges(counts: AuditCounts): { severity: AuditSeverity; count: number }[] {
  return auditSeverities.map((severity) => ({ severity, count: counts[severity] })).filter((badge) => badge.count > 0);
}

export function auditTotals(projects: readonly { audit?: AuditCounts | null }[]): AuditCounts | null {
  const audited = projects.flatMap((project) => (project.audit ? [project.audit] : []));
  if (audited.length === 0) return null;

  return audited.reduce(
    (sum, counts) => ({
      info: sum.info + counts.info,
      low: sum.low + counts.low,
      moderate: sum.moderate + counts.moderate,
      high: sum.high + counts.high,
      critical: sum.critical + counts.critical,
      total: sum.total + counts.total,
    }),
    { info: 0, low: 0, moderate: 0, high: 0, critical: 0, total: 0 },
  );
}
