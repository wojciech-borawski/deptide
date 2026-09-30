import type {
  FileSide,
  FileStatus,
  MergedFile,
  ReceiveFile,
  ReceivePlan,
  ReceiveProjectPlan,
  ReceiveRequest,
  ReceiveResult,
  ReceiveSelection,
} from "@/api/types";
import { mockReceiveFile } from "./receive-files";
import { fakeRoot, projectsRoot } from "./workspace";

function entry(relative: string, status: FileStatus, size: number): ReceiveFile {
  return { relative, status, size };
}

const richFiles: ReceiveFile[] = [
  entry("README.md", "identical", 300),
  entry("package.json", "replaced", 902),
  entry("tsconfig.json", "identical", 410),
  entry("config/app.yml", "replaced", 160),
  entry("docs/setup.md", "identical", 1200),
  entry("docs/api/endpoints.md", "identical", 2400),
  entry("public/favicon.ico", "identical", 4286),
  entry("public/logo.png", "replaced", 21_877),
  entry("src/App.vue", "replaced", 4210),
  entry("src/main.ts", "whitespace", 640),
  entry("src/legacy.ts", "removed", 410),
  entry("src/router/index.ts", "identical", 980),
  entry("src/components/NewPanel.vue", "added", 1980),
  entry("src/components/OldPanel.vue", "removed", 1620),
  entry("src/components/forms/AddressForm.vue", "replaced", 3050),
  entry("src/components/forms/inputs/DateInput.vue", "added", 1420),
  entry("src/components/forms/inputs/LegacyInput.vue", "removed", 870),
  entry("src/components/forms/inputs/TextInput.vue", "identical", 760),
  entry("src/generated/icons.ts", "replaced", 330_000),
  entry("src/generated/routes.ts", "replaced", 640_000),
  entry("src/generated/schema.json", "replaced", 2_480_000),
  entry("src/styles/reset.css", "identical", 520),
  entry("src/styles/theme.css", "whitespace", 2210),
  entry("tests/unit/app.test.ts", "added", 1330),
];

const smallFiles: ReceiveFile[] = [
  entry("package.json", "identical", 870),
  entry("src/index.ts", "replaced", 1210),
  entry("src/handlers/orders.ts", "replaced", 2890),
  entry("src/handlers/refunds.ts", "added", 1760),
];

const identicalFiles: ReceiveFile[] = [
  entry("package.json", "identical", 780),
  entry("src/index.ts", "identical", 1500),
];

function filesFor(target: string, disabled: ReadonlySet<string>): ReceiveFile[] {
  const base = target === "Orders-Api" ? identicalFiles : target === "Shop-Admin" ? richFiles : smallFiles;
  if (base === identicalFiles) return base;

  const extra: ReceiveFile[] = [];
  if (disabled.has("coverage/")) extra.push(entry("coverage/lcov.info", "added", 18_400));
  if (disabled.has(".env.local")) extra.push(entry(".env.local", "replaced", 120));
  return [...extra, ...base];
}

function projectPlan(request: ReceiveRequest, disabled: ReadonlySet<string>): ReceiveProjectPlan {
  const files = filesFor(request.target, disabled);
  const count = (status: FileStatus) => files.filter((file) => file.status === status).length;
  return {
    source: request.source,
    target: request.target,
    targetDirectory: `${projectsRoot}\\${request.target}`,
    files,
    skipped: 5 - disabled.size,
    added: count("added"),
    replaced: count("replaced"),
    whitespace: count("whitespace"),
    identical: count("identical"),
    removed: count("removed"),
  };
}

export function mockFileStatus(target: string, relative: string): FileStatus {
  return (
    filesFor(target, new Set([".env.local", "coverage/"])).find((file) => file.relative === relative)?.status ?? "added"
  );
}

export function mockReceivePlan(requests: ReceiveRequest[], disabledPatterns: string[]): ReceivePlan {
  const disabled = new Set(disabledPatterns);
  return { projects: requests.map((request) => projectPlan(request, disabled)) };
}

function sha256Of(side: FileSide | null): string | null {
  return side?.kind === "text" ? side.sha256 : null;
}

/** Merges whose hashes still match the mock files, and the paths of those that do not. */
function mockMerges(selection: ReceiveSelection): { merged: MergedFile[]; stale: string[] } {
  const merged: MergedFile[] = [];
  const stale: string[] = [];
  for (const merge of selection.merges ?? []) {
    const contents = mockReceiveFile(merge.relative, mockFileStatus(selection.target, merge.relative));
    const current =
      sha256Of(contents.local) === merge.localSha256 && sha256Of(contents.received) === merge.receivedSha256;
    if (current) {
      merged.push({ relative: merge.relative, taken: merge.chunks.length, total: merge.total, chunks: merge.chunks });
    } else {
      stale.push(merge.relative);
    }
  }
  return { merged, stale };
}

export function mockReceiveResult(selections: ReceiveSelection[]): ReceiveResult {
  const projects = selections.map((selection) => {
    const statuses = new Map(filesFor(selection.target, new Set()).map((file) => [file.relative, file.status]));
    const { merged, stale } = mockMerges(selection);
    const mergePaths = new Set((selection.merges ?? []).map((merge) => merge.relative));
    const skipped = selection.files.filter((file) => file.startsWith(".env"));
    const copied = selection.files.filter((file) => !skipped.includes(file) && !mergePaths.has(file));
    const added = copied.filter((file) => (statuses.get(file) ?? "added") === "added").length;
    const replaced = copied.length - added + merged.length;
    const files = [...copied, ...merged.map((file) => file.relative)];
    return {
      target: selection.target,
      targetDirectory: `${projectsRoot}\\${selection.target}`,
      added,
      replaced,
      deleted: selection.delete.length,
      recycled: replaced + selection.delete.length,
      bytes: files.length * 2000,
      files,
      deletedFiles: selection.delete,
      skipped,
      merged,
      stale,
    };
  });

  return {
    projects,
    files: projects.reduce((sum, project) => sum + project.files.length, 0),
    bytes: projects.reduce((sum, project) => sum + project.bytes, 0),
    receivedAt: new Date().toISOString(),
    logFile: `${fakeRoot}\\transfers\\receive.json`,
  };
}
