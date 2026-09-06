import { save as saveDialog } from "@tauri-apps/plugin-dialog";

import { renderRunReport, saveTextFile, type ReportFormat } from "@/api/commands";
import type { RunSummary } from "@/api/types";

const extensions: Record<ReportFormat, string> = { markdown: "md", html: "html" };

function fileStem(summary: RunSummary): string {
  const date = summary.startedAt.slice(0, 10);
  const label = summary.label
    .toLowerCase()
    .replace(/[^a-z0-9._-]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return `deptide-${date}-${label || "run"}`;
}

export async function copyReport(summary: RunSummary, format: ReportFormat): Promise<void> {
  const content = await renderRunReport(summary, format);
  await navigator.clipboard.writeText(content);
}

export async function saveReport(summary: RunSummary, format: ReportFormat): Promise<string | null> {
  const extension = extensions[format];
  const path = await saveDialog({
    title: "Save run report",
    defaultPath: `${fileStem(summary)}.${extension}`,
    filters: [{ name: format === "markdown" ? "Markdown" : "HTML", extensions: [extension] }],
  });

  if (!path) return null;

  const content = await renderRunReport(summary, format);
  await saveTextFile(path, content);
  return path;
}
