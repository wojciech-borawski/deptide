import { invoke } from "@tauri-apps/api/core";

import type {
  AppInfo,
  ClipboardContents,
  CopyResult,
  DetectedProject,
  ProjectInspection,
  ReceivePlan,
  ReceiveRequest,
  ReceiveResult,
  ReceiveSelection,
  RecentWorkspace,
  RunPlan,
  RunSnapshot,
  RunStartOutcome,
  RunSummary,
  ScanResult,
  ScanSelectionOutcome,
  Settings,
  TransferPreview,
  UpdateConfig,
  UpdateInfo,
  WorkspaceSnapshot,
} from "./types";

export type ReportFormat = "markdown" | "html";

export function reportError(source: string, message: string): void {
  invoke("log_client_error", { source, message }).catch(() => undefined);
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (cause) {
    reportError(`command ${command}`, String(cause));
    throw cause;
  }
}

export function listRecentWorkspaces(): Promise<RecentWorkspace[]> {
  return call("list_recent_workspaces");
}

export function openWorkspace(path: string): Promise<WorkspaceSnapshot> {
  return call("open_workspace", { path });
}

export function saveSettings(root: string, settings: Settings): Promise<Settings> {
  return call("save_settings", { root, settings });
}

export function saveConfig(root: string, config: UpdateConfig): Promise<WorkspaceSnapshot> {
  return call("save_config", { root, config });
}

export function scanProjects(root: string): Promise<ScanResult> {
  return call("scan_projects", { root });
}

export function applyScan(
  root: string,
  offered: DetectedProject[],
  selectedDirectories: string[],
): Promise<ScanSelectionOutcome> {
  return call("apply_scan", { root, offered, selectedDirectories });
}

export function inspectProjects(root: string, projectNames: string[]): Promise<ProjectInspection> {
  return call("inspect_projects", { root, projectNames });
}

export function startRun(root: string, plan: RunPlan): Promise<RunStartOutcome> {
  return call("start_run", { root, plan });
}

export function startCommandRun(
  root: string,
  projectNames: string[],
  command: string,
  concurrency: number,
): Promise<RunStartOutcome> {
  return call("start_command_run", { root, projectNames, command, concurrency });
}

export function abortRun(runId: string): Promise<void> {
  return call("abort_run", { runId });
}

export function abortJob(runId: string, jobName: string): Promise<void> {
  return call("abort_job", { runId, jobName });
}

export function restoreProject(runId: string, jobName: string): Promise<string[]> {
  return call("restore_project", { runId, jobName });
}

export function checkForUpdate(url: string): Promise<UpdateInfo> {
  return call("check_for_update", { url });
}

export function getRunSnapshot(runId: string): Promise<RunSnapshot> {
  return call("get_run_snapshot", { runId });
}

export function deleteSavedRun(root: string, name: string): Promise<void> {
  return call("delete_saved_run", { root, name });
}

export function suggestRunLabel(packageNames: string[]): Promise<string> {
  return call("suggest_run_label", { packageNames });
}

export function renderRunReport(summary: RunSummary, format: ReportFormat): Promise<string> {
  return call("render_run_report", { summary, format });
}

export function saveTextFile(path: string, content: string): Promise<void> {
  return call("save_text_file", { path, content });
}

export function errorLogPath(): Promise<string | null> {
  return call("error_log_path");
}

export function appInfo(): Promise<AppInfo> {
  return call("app_info");
}

export function startupWorkspace(): Promise<string | null> {
  return call("startup_workspace");
}

export function previewTransfer(
  root: string,
  projectNames: string[],
  extraPatterns: string[],
): Promise<TransferPreview> {
  return call("preview_transfer", { root, projectNames, extraPatterns });
}

export function copyProjectsToClipboard(
  root: string,
  projectNames: string[],
  extraPatterns: string[],
): Promise<CopyResult> {
  return call("copy_projects_to_clipboard", { root, projectNames, extraPatterns });
}

export function inspectClipboard(root: string): Promise<ClipboardContents> {
  return call("inspect_clipboard", { root });
}

export function analyzeReceive(
  root: string,
  requests: ReceiveRequest[],
  extraPatterns: string[],
): Promise<ReceivePlan> {
  return call("analyze_receive", { root, requests, extraPatterns });
}

export function applyReceive(root: string, selections: ReceiveSelection[]): Promise<ReceiveResult> {
  return call("apply_receive", { root, selections });
}
