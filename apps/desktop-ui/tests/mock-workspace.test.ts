import { afterEach, describe, expect, it, vi } from "vitest";

import type { ProjectRestore, RunBackup, RunPlan, RunStartOutcome, WorkspaceSnapshot } from "@/api/types";
import { handleMockCommand } from "@/dev/mock/handlers";
import { fakeRoot, workspaceState } from "@/dev/mock/workspace";

describe("mock workspace backend", () => {
  it("drops group members that are no longer configured when the config is saved, like the real backend", async () => {
    const config = {
      ...workspaceState.config,
      projects: workspaceState.config.projects.filter((project) => project.name !== "Shop-Admin"),
      groups: [
        { name: "shop", projects: ["Shop-Frontend", "Shop-Admin"] },
        { name: "admin only", projects: ["Shop-Admin"] },
      ],
    };

    const snapshot = (await handleMockCommand("save_config", { root: fakeRoot, config })) as WorkspaceSnapshot;

    expect(snapshot.config.groups).toEqual([{ name: "shop", projects: ["Shop-Frontend"] }]);
  });
});

const plan: RunPlan = {
  projectNames: ["Shop-Frontend", "Orders-Api"],
  packages: [{ name: "@demo/core", version: "2.3.0", saveDev: false, savePeer: false }],
  steps: ["install"],
  mode: "per-project",
  concurrency: 2,
  dryRun: false,
  extraInstallArgs: [],
  label: "core 2.3.0",
  saveAs: null,
  version: { bump: "patch", when: "always" },
  projectBumps: {},
};

async function finishedRun(dryRun: boolean): Promise<string | null> {
  const started = (await handleMockCommand("start_run", {
    root: fakeRoot,
    plan: { ...plan, dryRun },
  })) as RunStartOutcome;
  await vi.runAllTimersAsync();
  return (
    workspaceState.history.find((summary) => summary.startedAt === new Date(started.snapshot.startedAtMs).toISOString())
      ?.logFile ?? null
  );
}

describe("mock run backups", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("keeps a backup for each real run, none for a dry run, and restores the projects asked for", async () => {
    vi.useFakeTimers();
    const realLog = await finishedRun(false);
    const dryLog = await finishedRun(true);

    const backups = (await handleMockCommand("list_run_backups", { root: fakeRoot })) as RunBackup[];
    const real = backups.find((backup) => backup.logFile === realLog);

    expect(real?.projects.map((project) => project.name)).toEqual(["Shop-Frontend", "Orders-Api"]);
    expect(backups.some((backup) => backup.logFile === dryLog)).toBe(false);
    const results = (await handleMockCommand("restore_run_files", {
      root: fakeRoot,
      run: real?.run,
      projects: ["Orders-Api"],
    })) as ProjectRestore[];
    expect(results).toEqual([
      { name: "Orders-Api", outcome: "restored", files: ["package.json", "package-lock.json"] },
    ]);
  });
});
