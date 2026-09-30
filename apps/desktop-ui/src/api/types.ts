export type StepName = "uninstall" | "install" | "force-install" | "version" | "audit" | "build";

export type VersionBump = "patch" | "minor" | "major";

export const allBumps: readonly VersionBump[] = ["patch", "minor", "major"];

/** When the version step bumps a project's own version. */
export type BumpWhen = "always" | "same-as-main" | "not-bumped-on-branch";

export const allBumpWhens: readonly BumpWhen[] = ["always", "same-as-main", "not-bumped-on-branch"];

export interface VersionPolicy {
  bump: VersionBump;
  when: BumpWhen;
}

export const defaultVersionPolicy: VersionPolicy = { bump: "patch", when: "always" };

export type ExecutionMode = "per-project" | "per-step";

export type ProjectKind = "library" | "application" | "unknown";

export type JobStatus = "pending" | "running" | "ok" | "warn" | "failed" | "skipped";

export interface PackageSpec {
  name: string;
  version: string;
  saveDev: boolean;
  savePeer: boolean;
}

/** A package.json dependency field, listed in grouping precedence order. */
export type DependencySection = "dependencies" | "peer" | "dev";

export const allSections: readonly DependencySection[] = ["dependencies", "peer", "dev"];

export interface DependencyCandidate {
  name: string;
  localVersion: string | null;
  localBranch: string | null;
  branchSuffix: string | null;
  currentRanges: string[];
  usedBy: string[];
  isDevDependency: boolean;
  sections: DependencySection[];
}

export interface ConfiguredProject {
  name: string;
  path: string;
  packages?: string[];
  skip?: boolean;
  installArgs?: string[];
}

export interface UpdateConfig {
  packages: PackageSpec[];
  projects: ConfiguredProject[];
  installArgs?: string[];
  auditFixArgs?: string[];
  transferIgnore?: string[];
}

export interface Settings {
  projectsRoot: string;
  scanDepth: number;
  concurrency: number;
  steps: StepName[];
  mode: ExecutionMode;
  extraIgnoredDirectories: string[];
  branchSuffixPattern: string;
}

export interface ProjectView {
  name: string;
  path: string;
  absolutePath: string;
  exists: boolean;
  kind: ProjectKind;
  skip: boolean;
  packages: string[] | null;
  installArgs: string[] | null;
  duplicateOf: string[];
}

export interface SavedRun {
  name: string;
  savedAt: string;
  projects: string[];
  packages: PackageSpec[];
  steps: StepName[];
  concurrency: number;
  mode: ExecutionMode;
  extraInstallArgs: string[];
  version: VersionPolicy;
}

export interface SavedRunFile {
  fileName: string;
  filePath: string;
  run: SavedRun;
}

export interface StepTiming {
  step: StepName;
  durationMs: number;
}

export interface Diagnosis {
  code: string;
  title: string;
  hint: string;
}

export interface DependencyChange {
  name: string;
  before: string | null;
  after: string | null;
}

export interface AppInfo {
  name: string;
  version: string;
  identifier: string;
}

export interface UpdateInfo {
  current: string;
  latest: string;
  newer: boolean;
  url: string | null;
  notes: string | null;
}

export interface InstalledPackage {
  name: string;
  expected: string;
  installed: string | null;
  integrity: string | null;
  resolved: string | null;
  matches: boolean;
}

export interface RunProjectSummary {
  name: string;
  directory: string;
  status: JobStatus;
  durationMs: number;
  stepTimings: StepTiming[];
  error: string;
  warning: string;
  installed: InstalledPackage[];
  diagnosis: Diagnosis | null;
  dependencyChanges: DependencyChange[];
  retries: number;
}

export interface RunSummary {
  label: string;
  packages: string[];
  projectCount: number;
  concurrency: number;
  steps: StepName[];
  mode: ExecutionMode;
  dryRun: boolean;
  startedAt: string;
  finishedAt: string;
  totalDurationMs: number;
  busyDurationMs: number;
  logFile: string | null;
  projects: RunProjectSummary[];
  okCount: number;
  warnCount: number;
  failedCount: number;
  skippedCount: number;
  aborted: boolean;
}

export interface TransferProjectSummary {
  name: string;
  directory: string;
  files: number;
  bytes: number;
  skipped: number;
  stagedPath: string | null;
}

export interface TransferPreview {
  projects: TransferProjectSummary[];
  files: number;
  bytes: number;
  patterns: string[];
}

export interface CopyResult {
  stagingDirectory: string;
  projects: TransferProjectSummary[];
  files: number;
  bytes: number;
  copiedAt: string;
  logFile: string | null;
}

export interface ClipboardEntry {
  path: string;
  name: string;
  isProject: boolean;
  packageName: string | null;
  suggestedProject: string | null;
  files: number | null;
  bytes: number | null;
}

export type ClipboardSource = "paths" | "virtual" | "empty" | "busy" | "unreadable";

export interface ClipboardContents {
  source: ClipboardSource;
  sequence: number | null;
  entries: ClipboardEntry[];
  rejected: number;
  problem: string | null;
}

export interface ExtractProgress {
  filesDone: number;
  filesTotal: number;
  bytesDone: number;
  bytesTotal: number | null;
}

export interface DownloadedFolder {
  id: string;
  path: string;
}

export interface ClipboardDownload {
  sequence: number;
  directory: string;
  folders: DownloadedFolder[];
}

export type FileStatus = "added" | "replaced" | "whitespace" | "identical" | "removed";

export interface ReceiveFile {
  relative: string;
  status: FileStatus;
  /** Size of the received file, or of the target file for `removed`. */
  size: number;
}

export interface ReceiveProjectPlan {
  source: string;
  target: string;
  targetDirectory: string;
  files: ReceiveFile[];
  skipped: number;
  added: number;
  replaced: number;
  whitespace: number;
  identical: number;
  removed: number;
}

export type FileSide =
  | {
      kind: "text";
      /** Without the byte order mark; invalid UTF-8 replaced. */
      text: string;
      bom: boolean;
      size: number;
      /** Lowercase hex SHA-256 of the whole file, BOM included. */
      sha256: string;
      /** Whether the bytes after the BOM are valid UTF-8. */
      utf8: boolean;
    }
  | { kind: "binary"; size: number }
  | { kind: "tooLarge"; size: number };

export interface ReceiveFileContents {
  /** The file in the received folder, `null` when it has no such file. */
  received: FileSide | null;
  /** The file in the target project, `null` when it has no such file. */
  local: FileSide | null;
}

export interface ReceivePlan {
  projects: ReceiveProjectPlan[];
}

export interface ReceiveRequest {
  source: string;
  target: string;
}

/** One changed run of a line diff; 0-based line indexes after the BOM, old = target file, new = received file. */
export interface LineChunk {
  oldStart: number;
  oldCount: number;
  newStart: number;
  newCount: number;
}

export interface FileMerge {
  relative: string;
  /** `sha256` of the received side the chunks were computed from. */
  receivedSha256: string;
  /** `sha256` of the target side the chunks were computed from. */
  localSha256: string;
  /** How many chunks the diff had. */
  total: number;
  /** The chunks to take, in file order. */
  chunks: LineChunk[];
}

export interface ReceiveSelection {
  source: string;
  target: string;
  files: string[];
  /** Target files to move to the Recycle Bin; each must be absent from `source`. */
  delete: string[];
  /** Files rebuilt from some chunks of the received file; a path here is never copied whole. */
  merges?: FileMerge[];
}

export interface MergedFile {
  relative: string;
  taken: number;
  total: number;
  /** The chunks that were taken. */
  chunks: LineChunk[];
}

export interface ReceiveProjectResult {
  target: string;
  targetDirectory: string;
  added: number;
  replaced: number;
  deleted: number;
  /** Overwritten plus deleted files moved to the Recycle Bin. */
  recycled: number;
  bytes: number;
  files: string[];
  deletedFiles: string[];
  /** Entries of `files`, `delete` or `merges` that failed validation and were left alone. */
  skipped: string[];
  /** Merges written; their paths are in `files` and counted in `replaced`. */
  merged: MergedFile[];
  /** Merges left alone because either file is gone or no longer has the hash sent. */
  stale: string[];
}

export interface ReceiveResult {
  projects: ReceiveProjectResult[];
  files: number;
  bytes: number;
  receivedAt: string;
  logFile: string | null;
}

export interface RecentWorkspace {
  path: string;
  lastOpened: string;
}

export interface WorkspaceSnapshot {
  root: string;
  configFile: string;
  config: UpdateConfig;
  projects: ProjectView[];
  settings: Settings;
  savedRuns: SavedRunFile[];
  history: RunSummary[];
  recent: RecentWorkspace[];
}

export interface DetectedProject {
  directory: string;
  relativePath: string;
  proposedName: string;
  packageName: string | null;
  version: string | null;
  branch: string | null;
  kind: ProjectKind;
  dependencies: Record<string, string>;
  devDependencies: Record<string, string>;
  peerDependencies: Record<string, string>;
  hasBuildScript: boolean;
}

export interface ScannedProject extends DetectedProject {
  configPath: string;
  configuredName: string | null;
}

export interface ScanResult {
  projectsRoot: string;
  depth: number;
  projects: ScannedProject[];
}

export interface ScanSelectionOutcome {
  config: UpdateConfig;
  added: string[];
  removed: string[];
}

export interface ProjectInspection {
  candidates: DependencyCandidate[];
  unreadable: string[];
}

export interface RunPlan {
  projectNames: string[];
  packages: PackageSpec[];
  steps: StepName[];
  mode: ExecutionMode;
  concurrency: number;
  dryRun: boolean;
  extraInstallArgs: string[];
  label: string;
  saveAs: string | null;
  version: VersionPolicy;
}

export interface RunStartOutcome {
  runId: string;
  snapshot: RunSnapshot;
  missing: string[];
  withoutPackages: string[];
  savedRunPath: string | null;
}

export interface JobSnapshot {
  name: string;
  directory: string;
  packages: string[];
  status: JobStatus;
  currentStep: string;
  startedAtMs: number | null;
  stepStartedAtMs: number | null;
  durationMs: number;
  stepTimings: StepTiming[];
  error: string;
  warning: string;
  dependsOn: string[];
  installed: InstalledPackage[];
  diagnosis: Diagnosis | null;
  dependencyChanges: DependencyChange[];
  retries: number;
  backupAvailable: boolean;
  log: string[];
}

export interface RunSnapshot {
  id: string;
  label: string;
  packages: string[];
  steps: StepName[];
  mode: ExecutionMode;
  concurrency: number;
  dryRun: boolean;
  startedAtMs: number;
  finishedAtMs: number | null;
  currentPhase: StepName | null;
  aborted: boolean;
  command: string | null;
  jobs: JobSnapshot[];
  summary: RunSummary | null;
}

export type RunEvent =
  | { type: "jobChanged"; runId: string; job: JobSnapshot }
  | { type: "logLine"; runId: string; job: string; line: string }
  | { type: "phaseChanged"; runId: string; phase: StepName | null }
  | { type: "finished"; runId: string; snapshot: RunSnapshot };

export const allSteps: readonly StepName[] = ["uninstall", "install", "force-install", "version", "audit", "build"];

export const fullSteps: readonly StepName[] = ["uninstall", "install", "audit", "build"];

export const simpleSteps: readonly StepName[] = ["uninstall", "install"];

export const forceSteps: readonly StepName[] = ["force-install", "audit", "build"];
