import { describe, expect, it } from "vitest";

import type { DownloadedFolder } from "@/api/types";
import { clipboardIdFor, downloadPercent, downloadedFolder, toDownloadedRequests } from "@/lib/clipboard-download";

const folders: DownloadedFolder[] = [
  { id: "virtual:7/api", path: "C:\\recv\\7\\api" },
  { id: "virtual:7/web", path: "C:\\recv\\7\\web" },
];

describe("toDownloadedRequests", () => {
  it("swaps each clipboard id for the folder downloaded under that id, whatever the order", () => {
    const mapped = toDownloadedRequests(
      [
        { source: "virtual:7/web", target: "Web" },
        { source: "virtual:7/api", target: "Api" },
      ],
      folders,
    );

    expect(mapped).toEqual({
      requests: [
        { source: "C:\\recv\\7\\web", target: "Web" },
        { source: "C:\\recv\\7\\api", target: "Api" },
      ],
      missing: [],
    });
  });

  it("reports every id without a downloaded folder instead of dropping or guessing it", () => {
    const mapped = toDownloadedRequests(
      [
        { source: "virtual:7/web", target: "Web" },
        { source: "virtual:8/api", target: "Api" },
        { source: "virtual:7/docs", target: "Docs" },
      ],
      folders,
    );

    expect(mapped.missing).toEqual(["virtual:8/api", "virtual:7/docs"]);
    expect(mapped.requests).toEqual([{ source: "C:\\recv\\7\\web", target: "Web" }]);
  });
});

describe("downloadedFolder and clipboardIdFor", () => {
  it("finds the folder of an id and the id of a folder", () => {
    expect(downloadedFolder("virtual:7/web", folders)).toBe("C:\\recv\\7\\web");
    expect(downloadedFolder("virtual:7/docs", folders)).toBeNull();
    expect(clipboardIdFor("C:\\recv\\7\\api", folders)).toBe("virtual:7/api");
  });

  it("keeps a path that is not a downloaded folder", () => {
    expect(clipboardIdFor("C:\\clip\\web", folders)).toBe("C:\\clip\\web");
    expect(clipboardIdFor("C:\\clip\\web", [])).toBe("C:\\clip\\web");
  });
});

describe("downloadPercent", () => {
  it("follows the bytes when the total size is known", () => {
    expect(downloadPercent({ filesDone: 1, filesTotal: 4, bytesDone: 300, bytesTotal: 1200 })).toBe(25);
    expect(downloadPercent({ filesDone: 4, filesTotal: 4, bytesDone: 1300, bytesTotal: 1200 })).toBe(100);
  });

  it("follows the files when every file is empty", () => {
    expect(downloadPercent({ filesDone: 1, filesTotal: 2, bytesDone: 0, bytesTotal: 0 })).toBe(50);
    expect(downloadPercent({ filesDone: 0, filesTotal: 0, bytesDone: 0, bytesTotal: 0 })).toBe(100);
    expect(downloadPercent({ filesDone: 3, filesTotal: 2, bytesDone: 0, bytesTotal: 0 })).toBe(100);
  });

  it("has no percentage while the total size is unknown", () => {
    expect(downloadPercent({ filesDone: 3, filesTotal: 9, bytesDone: 500, bytesTotal: null })).toBeNull();
  });
});
