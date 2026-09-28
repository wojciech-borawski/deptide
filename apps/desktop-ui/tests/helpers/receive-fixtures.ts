import type { FileStatus, ReceiveFile, ReceiveProjectPlan } from "@/api/types";

export function file(relative: string, status: FileStatus, size = 100): ReceiveFile {
  return { relative, status, size };
}

export function makeProject(source: string, target: string, files: ReceiveFile[]): ReceiveProjectPlan {
  const count = (status: FileStatus) => files.filter((entry) => entry.status === status).length;
  return {
    source,
    target,
    targetDirectory: `C:\\repos\\${target}`,
    files,
    skipped: 0,
    added: count("added"),
    replaced: count("replaced"),
    whitespace: count("whitespace"),
    identical: count("identical"),
    removed: count("removed"),
  };
}
