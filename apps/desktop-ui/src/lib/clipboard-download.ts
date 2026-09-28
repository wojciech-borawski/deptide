import type { DownloadedFolder, ExtractProgress, ReceiveRequest } from "@/api/types";

export interface DownloadedRequests {
  requests: ReceiveRequest[];
  /** Clipboard ids of the requests that have no downloaded folder. */
  missing: string[];
}

/** Replaces each request's clipboard id with the folder downloaded under that id; ids are never matched by position. */
export function toDownloadedRequests(
  requests: readonly ReceiveRequest[],
  folders: readonly DownloadedFolder[],
): DownloadedRequests {
  const paths = new Map(folders.map((folder) => [folder.id, folder.path]));
  const mapped: ReceiveRequest[] = [];
  const missing: string[] = [];
  for (const request of requests) {
    const path = paths.get(request.source);
    if (path === undefined) missing.push(request.source);
    else mapped.push({ source: path, target: request.target });
  }
  return { requests: mapped, missing };
}

export function downloadedFolder(id: string, folders: readonly DownloadedFolder[]): string | null {
  return folders.find((folder) => folder.id === id)?.path ?? null;
}

/** The clipboard id `path` was downloaded for, or `path` itself when it is not a downloaded folder. */
export function clipboardIdFor(path: string, folders: readonly DownloadedFolder[]): string {
  return folders.find((folder) => folder.path === path)?.id ?? path;
}

/** Share of the download done, 0 to 100, or `null` while the total size is unknown. */
export function downloadPercent(progress: ExtractProgress): number | null {
  const { filesDone, filesTotal, bytesDone, bytesTotal } = progress;
  if (bytesTotal === null) return null;
  if (bytesTotal > 0) return Math.min(100, (100 * bytesDone) / bytesTotal);
  return filesTotal > 0 ? Math.min(100, (100 * filesDone) / filesTotal) : 100;
}
