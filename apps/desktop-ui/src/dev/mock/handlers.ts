import type { RunPlan, RunSnapshot, RunSummary, ScannedProject, Settings, UpdateConfig } from "@/api/types";
import { registerListener } from "./events";
import { MockRun } from "./run";
import {
  delay,
  dependencyCandidates,
  fakeRoot,
  projectsRoot,
  scannedProjects,
  workspaceSnapshot,
  workspaceState,
} from "./workspace";

const runs = new Map<string, MockRun>();

function launch(run: MockRun, missing: string[]) {
  runs.set(run.id, run);
  const snapshot = JSON.parse(JSON.stringify(run.state)) as RunSnapshot;
  void run.execute();
  return { runId: run.id, snapshot, missing, withoutPackages: [], savedRunPath: null };
}

function applyScan(args: Record<string, unknown>) {
  const selected = new Set(args.selectedDirectories as string[]);
  const offered = args.offered as ScannedProject[];
  const config = workspaceState.config;
  const kept = config.projects.filter((project) => {
    const match = offered.find((entry) => entry.configPath === project.path);
    return !match || selected.has(match.directory);
  });
  const added = offered
    .filter((entry) => selected.has(entry.directory) && !config.projects.some((p) => p.path === entry.configPath))
    .map((entry) => ({ name: entry.proposedName, path: entry.configPath }));
  const removed = config.projects.filter((project) => !kept.includes(project)).map((p) => p.name);
  workspaceState.config = { ...config, projects: [...kept, ...added] };
  return { config: workspaceState.config, added: added.map((p) => p.name), removed };
}

function transferProjects(names: string[], stagedPrefix: string | null) {
  return names.map((name) => ({
    name,
    directory: `${projectsRoot}\\${name}`,
    files: 42,
    bytes: 812_000,
    skipped: 3,
    stagedPath: stagedPrefix ? `${stagedPrefix}\\${name}` : null,
  }));
}

export async function handleMockCommand(command: string, args: Record<string, unknown>): Promise<unknown> {
  switch (command) {
    case "plugin:event|listen":
      return registerListener(String(args.event), Number(args.handler));
    case "plugin:event|unlisten":
      return null;
    case "plugin:dialog|open":
      return fakeRoot;
    case "plugin:dialog|save":
      return `${fakeRoot}\\report.md`;
    case "plugin:opener|open_path":
    case "plugin:opener|reveal_item_in_dir":
      return null;
    case "list_recent_workspaces":
      return workspaceSnapshot().recent;
    case "open_workspace":
      await delay(200);
      return workspaceSnapshot();
    case "save_settings":
      Object.assign(workspaceState.settings, args.settings as Settings);
      return workspaceState.settings;
    case "save_config":
      workspaceState.config = args.config as UpdateConfig;
      return workspaceSnapshot();
    case "scan_projects":
      await delay(500);
      return { projectsRoot, depth: workspaceState.settings.scanDepth, projects: scannedProjects() };
    case "apply_scan":
      return applyScan(args);
    case "inspect_projects":
      await delay(400);
      return {
        candidates: dependencyCandidates(),
        unreadable: (args.projectNames as string[]).filter((name) => name.includes("Legacy")),
      };
    case "start_run":
      return launch(new MockRun(args.plan as RunPlan), []);
    case "start_command_run": {
      const names = args.projectNames as string[];
      const plan: RunPlan = {
        projectNames: names,
        packages: [],
        steps: [],
        mode: "per-project",
        concurrency: Number(args.concurrency),
        dryRun: false,
        extraInstallArgs: [],
        label: `cmd: ${String(args.command)}`,
        saveAs: null,
      };
      return launch(
        new MockRun(plan, String(args.command)),
        names.filter((name) => name.includes("Legacy")),
      );
    }
    case "abort_run":
      runs.get(String(args.runId))?.abort();
      return null;
    case "abort_job":
      runs.get(String(args.runId))?.abortJob(String(args.jobName));
      return null;
    case "restore_project":
      return ["package.json", "package-lock.json"];
    case "check_for_update":
      await delay(300);
      return {
        current: "1.0.0",
        latest: "1.1.0",
        newer: true,
        url: "https://example.com/deptide",
        notes: "Mock release",
      };
    case "get_run_snapshot":
      return runs.get(String(args.runId))?.state ?? Promise.reject(new Error("Unknown run"));
    case "delete_saved_run":
      return null;
    case "suggest_run_label":
      return "mock-run";
    case "render_run_report":
      return `# Deptide run: ${(args.summary as RunSummary).label}\n\nMock report body.`;
    case "save_text_file":
    case "log_client_error":
    case "startup_workspace":
      return null;
    case "app_info":
      return { name: "Deptide", version: "1.0.0", identifier: "dev.deptide.app" };
    case "preview_transfer": {
      await delay(300);
      const names = args.projectNames as string[];
      return {
        projects: transferProjects(names, null),
        files: 42 * names.length,
        bytes: 812_000 * names.length,
        patterns: [...(workspaceState.config.transferIgnore ?? []), ...(args.extraPatterns as string[])],
      };
    }
    case "copy_projects_to_clipboard": {
      await delay(600);
      const names = args.projectNames as string[];
      const stagingDirectory = "C:\\Temp\\deptide-transfer\\2026-09-06T14-00-00";
      return {
        stagingDirectory,
        projects: transferProjects(names, stagingDirectory),
        files: 42 * names.length,
        bytes: 812_000 * names.length,
        copiedAt: new Date().toISOString(),
        logFile: `${fakeRoot}\\transfers\\copy.json`,
      };
    }
    case "inspect_clipboard":
      return {
        entries: [
          {
            path: "C:\\Temp\\deptide-transfer\\x\\Shop-Admin",
            name: "Shop-Admin",
            isProject: true,
            packageName: "shop-admin",
            suggestedProject: "Shop-Admin",
          },
          {
            path: "C:\\Temp\\deptide-transfer\\x\\somewhere-else",
            name: "somewhere-else",
            isProject: true,
            packageName: "else",
            suggestedProject: null,
          },
        ],
      };
    case "analyze_receive":
      await delay(400);
      return {
        projects: (args.requests as { source: string; target: string }[]).map((request) => ({
          source: request.source,
          target: request.target,
          targetDirectory: `${projectsRoot}\\${request.target}`,
          files: [
            { relative: "src/App.vue", status: "replaced", size: 4210 },
            { relative: "src/components/NewPanel.vue", status: "added", size: 1980 },
            { relative: "package.json", status: "replaced", size: 902 },
            { relative: "README.md", status: "identical", size: 300 },
          ],
          skipped: 2,
          added: 1,
          replaced: 2,
          identical: 1,
        })),
      };
    case "apply_receive": {
      await delay(500);
      const selections = args.selections as { source: string; target: string; files: string[] }[];
      return {
        projects: selections.map((selection) => ({
          target: selection.target,
          targetDirectory: `${projectsRoot}\\${selection.target}`,
          added: selection.files.filter((file) => file.includes("New")).length,
          replaced: selection.files.filter((file) => !file.includes("New")).length,
          bytes: selection.files.length * 2000,
          files: selection.files,
        })),
        files: selections.reduce((sum, selection) => sum + selection.files.length, 0),
        bytes: 6000,
        receivedAt: new Date().toISOString(),
        logFile: `${fakeRoot}\\transfers\\receive.json`,
      };
    }
    case "error_log_path":
      return `${fakeRoot}\\logs\\deptide-errors.log`;
    default:
      throw new Error(`Mock backend: unknown command ${command}`);
  }
}
