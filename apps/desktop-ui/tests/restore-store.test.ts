import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

import type { RunBackup } from "@/api/types";
import { makeSnapshot, makeSummary } from "./helpers/run-fixtures";

const backend = vi.hoisted(() => ({ listRunBackups: vi.fn(), restoreRunFiles: vi.fn() }));

vi.mock("@/api/commands", () => ({ ...backend, reportError: () => undefined }));

import { useRestoreStore } from "@/stores/restore";

const kept: RunBackup = {
  run: "2026-10-22T09-30-00-acme-core",
  logFile: "C:\\ws\\logs\\2026-10-22T09-30-00-acme-core.log",
  folder: "2026-10-22T09-30-00-acme-core",
  projects: [{ name: "web", directory: "C:\\repos\\web", files: ["package.json", "package-lock.json"] }],
};

beforeEach(() => {
  setActivePinia(createPinia());
  backend.listRunBackups.mockReset();
  backend.restoreRunFiles.mockReset();
});

describe("run backups", () => {
  it("finds the backup of a run by its transcript and nothing for a run whose backups are gone", async () => {
    backend.listRunBackups.mockResolvedValue([kept]);
    const store = useRestoreStore();
    const snapshot = makeSnapshot("run-1", []);

    await store.load("C:\\ws");

    expect(backend.listRunBackups).toHaveBeenCalledWith("C:\\ws");
    expect(store.backupFor(makeSummary(snapshot, { logFile: kept.logFile }))).toEqual(kept);
    expect(store.backupFor(makeSummary(snapshot, { logFile: "C:\\ws\\logs\\pruned.log" }))).toBeNull();
    expect(store.backupFor(makeSummary(snapshot, { logFile: null }))).toBeNull();
  });

  it("keeps the newest listing when an older request answers last", async () => {
    const replies: Array<(backups: RunBackup[]) => void> = [];
    backend.listRunBackups.mockImplementation(() => new Promise((resolve) => replies.push(resolve)));
    const store = useRestoreStore();
    const older = store.load("C:\\ws");
    const newer = store.load("C:\\ws");

    replies[1]!([kept]);
    await newer;
    replies[0]!([]);
    await older;

    expect(store.backups).toEqual([kept]);
  });
});
