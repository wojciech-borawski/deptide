export type StepName =
  "uninstall" | "install" | "force-install" | "version" | "audit" | "audit-report" | "build" | "pack";

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

export type PackMode = "no-save" | "save";

export interface PackageSpec {
  name: string;
  version: string;
  saveDev: boolean;
  savePeer: boolean;
  pack?: PackMode;
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
  libraryProject: string | null;
  ambiguousProjects: string[];
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
  groups?: ProjectGroup[];
  installArgs?: string[];
  auditFixArgs?: string[];
  transferIgnore?: string[];
}

export interface ProjectGroup {
  name: string;
  projects: string[];
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
  /** Bump levels that differ from `version.bump`, by project name; absent in older files. */
  projectBumps?: Record<string, VersionBump>;
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

/** Vulnerability counts per severity from `npm audit --json`. */
export interface AuditCounts {
  info: number;
  low: number;
  moderate: number;
  high: number;
  critical: number;
  total: number;
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
  /** Present when the project ran the audit report step; absent in older summaries. */
  audit?: AuditCounts;
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
  backupFolder?: string;
}

export interface BackedUpProject {
  name: string;
  directory: string;
  files: string[];
}

export interface RunBackup {
  run: string;
  logFile: string;
  folder: string;
  projects: BackedUpProject[];
}

export type ProjectRestore =
  { name: string; outcome: "restored"; files: string[] } | { name: string; outcome: "failed"; error: string };

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

export interface DriftCell {
  project: string;
  section: DependencySection;
  declared: string;
  installed: string | null;
  outOfRange: boolean;
  differs: boolean;
  behind: boolean;
}

export interface LibraryDrift {
  name: string;
  localProject: string | null;
  localVersion: string | null;
  ambiguousProjects: string[];
  newestInstalled: string | null;
  latest: string | null;
  latestError: string | null;
  mismatch: boolean;
  cells: DriftCell[];
}

export interface DependencyDrift {
  projects: string[];
  unreadable: string[];
  libraries: LibraryDrift[];
}

export interface GraphNode {
  name: string;
  packageName: string | null;
  layer: number;
  dependsOn: string[];
}

export interface GraphEdge {
  consumer: string;
  library: string;
  section: DependencySection;
}

export interface DependencyGraph {
  nodes: GraphNode[];
  edges: GraphEdge[];
  cycles: string[][];
}

export type CopyScope = "libraries" | "all-packages";

export interface InstalledCopy {
  version: string | null;
  path: string;
}

export interface PackageCopies {
  name: string;
  copies: InstalledCopy[];
}

export interface ProjectCopies {
  project: string;
  hasNodeModules: boolean;
  packages: PackageCopies[];
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
  /** Bump levels that differ from `version.bump`, by project name. */
  projectBumps: Record<string, VersionBump>;
}

export type ReasonKind =
  | "stillEqualsMain"
  | "differsFromMain"
  | "mainUnreadable"
  | "notBumpedOnBranch"
  | "changedUncommitted"
  | "changedOnBranch"
  | "branchUnknown"
  | "lookupStopped"
  | "invalidVersion"
  | "manifestUnreadable"
  | "noBuildScript"
  | "saveExactDroppedForPeers"
  | "noLibraries"
  | "noProjectUsesPackages"
  | "packedNotUninstalled"
  | "builtBeforePacking"
  | "packedLibraryMissing"
  | "packedLibraryAmbiguous"
  | "onlyPacked"
  | "packedBeforeOwnChanges";

/** Why git could not tell what the current branch did to the version; the `cause` of a `branchUnknown` reason. */
export type LookupCause =
  | "notARepository"
  | "noCommits"
  | "detachedHead"
  | "onMainBranch"
  | "noReflog"
  | "noOtherBranch"
  | "branchListFailed"
  | "remoteListFailed"
  | "commitListFailed"
  | "shallowClone"
  | "noSharedHistory"
  | "noVersionAtBase"
  | "noVersionInLastCommit"
  | "historyUnreadable";

/** Why a project or step is skipped or fails. `text` is the English line the run logs. */
export interface ReasonPreview {
  kind: ReasonKind;
  /** The values the wording needs; a `version` of null means package.json has none, a `cause` is a {@link LookupCause}. */
  params: Record<string, string | null>;
  text: string;
}

/** What the version step will do to one project. `current` is null when package.json has no version. */
export type VersionPreview =
  | { decision: "bump"; current: string | null; next: string; level: VersionBump; reason: ReasonPreview | null }
  | { decision: "skip"; current: string | null; level: VersionBump; reason: ReasonPreview }
  | { decision: "fail"; current: string | null; level: VersionBump; reason: ReasonPreview };

export interface StepPreview {
  step: StepName;
  /** npm command lines in run order, as the run logs them. */
  commands: string[];
  /** What the run logs for the step besides its commands. */
  notes: ReasonPreview[];
}

export interface JobPreview {
  name: string;
  dependsOn: string[];
  steps: StepPreview[];
  version: VersionPreview | null;
}

/** What starting a plan would run, worked out without touching any project. */
export interface PlanPreview {
  jobs: JobPreview[];
  missing: string[];
  withoutPackages: string[];
  /** The error starting the run would fail with, when no job is left. */
  nothingToDo: ReasonPreview | null;
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
  audit: AuditCounts | null;
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

export const allSteps: readonly StepName[] = [
  "uninstall",
  "install",
  "force-install",
  "version",
  "audit",
  "audit-report",
  "build",
];

export const fullSteps: readonly StepName[] = ["uninstall", "install", "audit", "build"];

export const simpleSteps: readonly StepName[] = ["uninstall", "install"];

export const forceSteps: readonly StepName[] = ["force-install", "audit", "build"];
