import type { Channel } from "@tauri-apps/api/core";

import type { ClipboardContents, ClipboardDownload, ClipboardEntry, ExtractProgress } from "@/api/types";
import { delay } from "./workspace";

type ClipboardMode = "paths" | "virtual" | "virtual-fail";

const remoteSequence = 7;
const receivedDirectory = "C:\\Temp\\deptide-received\\2026-09-28T12-00-00";
const downloadSteps = 20;
const failingStep = 8;
const changed = "The clipboard changed, analyze again";

const pathEntries: ClipboardEntry[] = [
  onDisk("Shop-Admin", "shop-admin", "Shop-Admin"),
  onDisk("Shop-Frontend", "shop-frontend", "Shop-Frontend"),
  onDisk("orders-api", "orders-api", "Orders-Api"),
  onDisk("somewhere-else", "else", null),
];

const remoteEntries: ClipboardEntry[] = [
  {
    path: `virtual:${remoteSequence}/Shop-Admin`,
    name: "Shop-Admin",
    isProject: true,
    packageName: "shop-admin",
    suggestedProject: "Shop-Admin",
    files: 214,
    bytes: 3_482_113,
  },
  {
    path: `virtual:${remoteSequence}/Shop-Frontend`,
    name: "Shop-Frontend",
    isProject: false,
    packageName: null,
    suggestedProject: "Shop-Frontend",
    files: 57,
    bytes: 402_880,
  },
];

let cancelled = false;
let finished: ClipboardDownload | null = null;

function onDisk(name: string, packageName: string, suggestedProject: string | null): ClipboardEntry {
  return {
    path: `C:\\Temp\\deptide-transfer\\x\\${name}`,
    name,
    isProject: true,
    packageName,
    suggestedProject,
    files: null,
    bytes: null,
  };
}

function clipboardMode(): ClipboardMode {
  const mode = new URLSearchParams(window.location.search).get("clipboard");
  return mode === "virtual" || mode === "virtual-fail" ? mode : "paths";
}

export function mockClipboard(): ClipboardContents {
  if (clipboardMode() === "paths") {
    return { source: "paths", sequence: null, entries: pathEntries, rejected: 0, problem: null };
  }
  return { source: "virtual", sequence: remoteSequence, entries: remoteEntries, rejected: 1, problem: null };
}

export function cancelMockDownload(): void {
  cancelled = true;
}

export async function mockDownload(sequence: number, channel: Channel<ExtractProgress>): Promise<ClipboardDownload> {
  cancelled = false;
  if (sequence !== remoteSequence || clipboardMode() === "paths") throw changed;
  if (finished) return finished;

  const filesTotal = remoteEntries.reduce((sum, entry) => sum + (entry.files ?? 0), 0);
  const bytesTotal = remoteEntries.reduce((sum, entry) => sum + (entry.bytes ?? 0), 0);
  for (let step = 1; step <= downloadSteps; step += 1) {
    await delay(100);
    if (cancelled) throw "The download was cancelled";
    if (clipboardMode() === "virtual-fail" && step === failingStep) throw changed;
    channel.onmessage({
      filesDone: Math.round((filesTotal * step) / downloadSteps),
      filesTotal,
      bytesDone: Math.round((bytesTotal * step) / downloadSteps),
      bytesTotal,
    });
  }

  finished = {
    sequence,
    directory: receivedDirectory,
    folders: remoteEntries.map((entry) => ({ id: entry.path, path: `${receivedDirectory}\\${entry.name}` })),
  };
  return finished;
}
