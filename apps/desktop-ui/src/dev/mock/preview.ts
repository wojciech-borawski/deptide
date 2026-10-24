import type {
  AuditCounts,
  JobPreview,
  PackageSpec,
  PlanPreview,
  ReasonKind,
  ReasonPreview,
  RunPlan,
  StepName,
  StepPreview,
  VersionBump,
  VersionPreview,
} from "@/api/types";
import { i18n } from "@/i18n";
import { dependencyCandidates, fakeRoot, projectViews, runFileName, workspaceState } from "./workspace";
import { sha256Hex } from "./sha256";

interface MockProject {
  version: string;
  versionOnMain: string;
  bumpedOnBranch: { from: string; to: string; commit: string } | null;
  hasBuildScript: boolean;
  audit: AuditCounts;
}

const clean: AuditCounts = { info: 0, low: 0, moderate: 0, high: 0, critical: 0, total: 0 };

const mockProjects: Record<string, MockProject> = {
  "Shop-Frontend": {
    version: "1.4.2",
    versionOnMain: "1.4.2",
    bumpedOnBranch: null,
    hasBuildScript: true,
    audit: clean,
  },
  "Shop-Admin": {
    version: "2.0.1",
    versionOnMain: "2.0.0",
    bumpedOnBranch: { from: "2.0.0", to: "2.0.1", commit: "a1b2c3d" },
    hasBuildScript: true,
    audit: { info: 0, low: 1, moderate: 2, high: 0, critical: 0, total: 3 },
  },
  "Orders-Api": { version: "0.9.0", versionOnMain: "0.9.0", bumpedOnBranch: null, hasBuildScript: false, audit: clean },
  "Billing-Api": {
    version: "3.1.0-rc.1",
    versionOnMain: "3.1.0-rc.1",
    bumpedOnBranch: null,
    hasBuildScript: true,
    audit: { info: 0, low: 0, moderate: 1, high: 2, critical: 1, total: 4 },
  },
  "Demo-Core": {
    version: "2.3.0-DEMO-7",
    versionOnMain: "2.2.0",
    bumpedOnBranch: { from: "2.2.0", to: "2.3.0-DEMO-7", commit: "c4d5e6f" },
    hasBuildScript: true,
    audit: clean,
  },
};

const fallbackProject: MockProject = {
  version: "1.0.0",
  versionOnMain: "1.0.0",
  bumpedOnBranch: null,
  hasBuildScript: true,
  audit: clean,
};

const packageSteps: readonly StepName[] = ["uninstall", "install", "force-install"];
const shellSpecial = /[\s^&|<>()%!"';,$`{}]/;
const saveExactFlags: readonly string[] = ["--save-exact", "-E", "--save-exact=true"];
const libraryFolderSuffixLength = 8;

export interface MockJob {
  name: string;
  directory: string;
  packages: PackageSpec[];
  steps: StepName[];
  installArgs: string[];
  level: VersionBump;
  dependsOn: string[];
  packFolder: string | null;
  tarballs: Record<string, MockTarball>;
}

interface MockTarball {
  file: string;
  version: string;
}

function mockProject(name: string): MockProject {
  const own = Object.hasOwn(mockProjects, name) ? mockProjects[name] : undefined;
  return own ?? fallbackProject;
}

export function mockAudit(name: string): AuditCounts {
  return { ...mockProject(name).audit };
}

function mockReason(kind: ReasonKind, params: Record<string, string | null> = {}): ReasonPreview {
  const shown = Object.fromEntries(Object.entries(params).map(([key, value]) => [key, value ?? "none"]));
  return { kind, params, text: i18n.global.t(`review.reason.${kind}`, shown, { locale: "en" }) };
}

function quoteArg(arg: string): string {
  return arg === "" || shellSpecial.test(arg) ? `"${arg.replace(/"/g, '\\"')}"` : arg;
}

export function commandLine(args: string[]): string {
  return ["npm", ...args.map(quoteArg)].join(" ");
}

function installSection(name: string): "dev" | "peer" | "normal" {
  const sections = dependencyCandidates().find((candidate) => candidate.name === name)?.sections ?? [];
  if (sections.includes("dependencies")) return "normal";
  if (sections.includes("dev")) return "dev";
  return sections.includes("peer") ? "peer" : "normal";
}

function declares(project: string, library: string): boolean {
  return dependencyCandidates().some((candidate) => candidate.name === library && candidate.usedBy.includes(project));
}

function installArgs(own: string[] | null, plan: RunPlan): string[] {
  const args = [...(own ?? workspaceState.config.installArgs ?? [])];
  for (const extra of plan.extraInstallArgs) {
    if (!args.includes(extra)) args.push(extra);
  }
  return args;
}

export function mockJobs(plan: RunPlan): { jobs: MockJob[]; missing: string[]; withoutPackages: string[] } {
  const views = projectViews();
  const missing = plan.projectNames.filter((name) => !views.some((view) => view.name === name));
  const selected = plan.projectNames.flatMap((name) => views.filter((view) => view.name === name));
  const steps = plan.steps.filter((step) => plan.packages.length > 0 || !packageSteps.includes(step));
  if (steps.length === 0) return { jobs: [], missing, withoutPackages: [] };

  const declaredSomewhere = plan.packages.map((spec) => selected.some((view) => declares(view.name, spec.name)));
  const jobs: MockJob[] = [];
  const withoutPackages: string[] = [];

  for (const view of selected) {
    const packages = plan.packages
      .filter(
        (spec, index) =>
          (view.packages ?? [spec.name]).includes(spec.name) &&
          !(spec.pack && libraryOf(spec.name)?.project === view.name) &&
          (!view.exists || declares(view.name, spec.name) || !declaredSomewhere[index]),
      )
      .map((spec) => {
        if (!view.exists) return spec;
        const section = declares(view.name, spec.name) ? installSection(spec.name) : "normal";
        return { ...spec, saveDev: section === "dev", savePeer: section === "peer" };
      });
    if (packages.length === 0 && plan.packages.length > 0) {
      withoutPackages.push(view.name);
      continue;
    }
    const ownLevel = Object.hasOwn(plan.projectBumps, view.name) ? plan.projectBumps[view.name] : undefined;
    jobs.push({
      name: view.name,
      directory: view.absolutePath,
      packages,
      steps,
      installArgs: installArgs(view.installArgs, plan),
      level: ownLevel ?? plan.version.bump,
      dependsOn: [],
      packFolder: null,
      tarballs: {},
    });
  }
  addPackJobs(plan, jobs);
  return { jobs: orderByDependencies(jobs), missing, withoutPackages };
}

function orderByDependencies(jobs: MockJob[]): MockJob[] {
  const ordered: MockJob[] = [];
  let remaining = jobs;
  for (;;) {
    const placed = new Set(ordered.map((job) => job.name));
    const ready = remaining.filter((job) => job.dependsOn.every((name) => placed.has(name)));
    if (ready.length === 0) break;
    ordered.push(...ready);
    remaining = remaining.filter((job) => !ready.includes(job));
  }
  const placed = new Set(ordered.map((job) => job.name));
  for (const job of remaining) {
    job.dependsOn = job.dependsOn.filter((name) => placed.has(name));
    ordered.push(job);
  }
  return ordered;
}

function libraryOf(name: string): { project: string; version: string | null } | null {
  const candidate = dependencyCandidates().find((entry) => entry.name === name);
  return candidate?.libraryProject ? { project: candidate.libraryProject, version: candidate.localVersion } : null;
}

function tarballName(name: string, version: string): string {
  return `${name.replace(/^@/, "").replace("/", "-")}-${version}.tgz`;
}

function packedVersion(name: string, jobs: MockJob[], plan: RunPlan): string | null {
  const library = libraryOf(name);
  const own = jobs.find((job) => job.name === library?.project);
  if (!own || !runsBeforePacking(own, "version")) return library?.version ?? null;
  const version = previewVersion(own, plan);
  return version.decision === "bump" ? version.next : version.current;
}

function addPackJobs(plan: RunPlan, jobs: MockJob[]): void {
  const names = [
    ...new Set(plan.packages.filter((spec) => spec.pack && libraryOf(spec.name)).map((spec) => spec.name)),
  ];
  const projects = [...new Set(names.flatMap((name) => libraryOf(name)?.project ?? []))];
  if (projects.length === 0) return;
  const folder = `${fakeRoot}\\packs\\${runFileName(Date.now(), plan.label)}`;

  for (const project of projects) {
    const own = jobs.find((job) => job.name === project);
    if (own) {
      own.steps = plan.mode === "per-project" ? [...own.steps, "pack"] : ["pack", ...own.steps];
      continue;
    }
    jobs.push({
      name: project,
      directory: projectViews().find((view) => view.name === project)?.absolutePath ?? "",
      packages: [],
      steps: ["pack"],
      installArgs: [],
      level: plan.version.bump,
      dependsOn: [],
      packFolder: null,
      tarballs: {},
    });
  }

  const tarballs: Record<string, MockTarball> = {};
  for (const name of names) {
    const version = packedVersion(name, jobs, plan);
    if (version) tarballs[name] = { file: tarballName(name, version), version };
  }
  for (const job of jobs) {
    const packed = job.packages.filter((spec) => spec.pack);
    for (const project of packed.flatMap((spec) => libraryOf(spec.name)?.project ?? [])) {
      if (!job.dependsOn.includes(project)) job.dependsOn.push(project);
    }
    if (packed.length > 0 || job.steps.includes("pack")) {
      job.packFolder = folder;
      job.tarballs = tarballs;
    }
  }
}

function libraryFolder(job: MockJob, name: string): string {
  const folder = name.startsWith("@") ? name.slice(1).replace("/", "+") : name;
  return `${job.packFolder ?? ""}\\${folder}-${sha256Hex(name).slice(0, libraryFolderSuffixLength)}`;
}

function tarballPath(job: MockJob, name: string): string | null {
  const tarball = job.tarballs[name];
  return job.packFolder && tarball ? `${libraryFolder(job, name)}\\${tarball.file}` : null;
}

export function expectedVersion(job: MockJob, spec: PackageSpec): string {
  return (spec.pack && job.tarballs[spec.name]?.version) || spec.version;
}

function installArgument(job: MockJob, spec: PackageSpec): string | null {
  return spec.pack ? tarballPath(job, spec.name) : `${spec.name}@${spec.version}`;
}

export function nextVersion(current: string, bump: VersionBump): string | null {
  const match = /^[v=]?(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?$/.exec(current.trim());
  if (!match) return null;
  const [major, minor, patch] = [Number(match[1]), Number(match[2]), Number(match[3])];
  const prerelease = match[4] !== undefined;

  if (bump === "patch") return prerelease ? `${major}.${minor}.${patch}` : `${major}.${minor}.${patch + 1}`;
  if (bump === "minor") return prerelease && patch === 0 ? `${major}.${minor}.0` : `${major}.${minor + 1}.0`;
  return prerelease && minor === 0 && patch === 0 ? `${major}.0.0` : `${major + 1}.0.0`;
}

function previewVersion(job: MockJob, plan: RunPlan): VersionPreview {
  const project = mockProject(job.name);
  const current = project.version;
  const level = job.level;
  let reason: ReasonPreview | null = null;

  if (plan.version.when === "same-as-main") {
    if (current !== project.versionOnMain) {
      return {
        decision: "skip",
        current,
        level,
        reason: mockReason("differsFromMain", { version: current, main: project.versionOnMain }),
      };
    }
    reason = mockReason("stillEqualsMain", { version: current });
  } else if (plan.version.when === "not-bumped-on-branch") {
    if (project.bumpedOnBranch) {
      return {
        decision: "skip",
        current,
        level,
        reason: mockReason("changedOnBranch", { version: current, ...project.bumpedOnBranch }),
      };
    }
    reason = mockReason("notBumpedOnBranch", { version: current, base: "main", commit: "9f8e7d6" });
  }

  const next = nextVersion(current, level);
  return next === null
    ? { decision: "fail", current, level, reason: mockReason("invalidVersion", { version: current }) }
    : { decision: "bump", current, next, level, reason };
}

function versionNote(version: VersionPreview): ReasonPreview | null {
  if (version.decision === "skip") return { ...version.reason, text: `${version.reason.text}, bump skipped` };
  return version.reason === null ? null : { ...version.reason, text: `${version.reason.text}, bumping` };
}

function forceFlags(job: MockJob, force: boolean): string[] {
  return force && !job.installArgs.includes("--force") ? ["--force"] : [];
}

function unsavedInstall(job: MockJob, force: boolean): string[][] {
  const unsaved = job.packages.flatMap((spec) => (spec.pack === "no-save" ? (tarballPath(job, spec.name) ?? []) : []));
  return unsaved.length > 0
    ? [["install", ...unsaved, ...forceFlags(job, force), ...job.installArgs, "--no-save"]]
    : [];
}

function auditCommands(job: MockJob): string[][] {
  const installs = job.steps.includes("install") || job.steps.includes("force-install");
  return [["audit", "fix"], ...(installs ? unsavedInstall(job, job.steps.includes("force-install")) : [])];
}

function installCommands(job: MockJob, force: boolean): string[][] {
  const saved = job.packages.filter((spec) => spec.pack !== "no-save");
  const forceFlag = forceFlags(job, force);
  const groups: { flag: string | null; peer: boolean; specs: PackageSpec[] }[] = [
    { flag: null, peer: false, specs: saved.filter((spec) => !spec.saveDev && !spec.savePeer) },
    { flag: "--save-dev", peer: false, specs: saved.filter((spec) => spec.saveDev) },
    { flag: "--save-peer", peer: true, specs: saved.filter((spec) => !spec.saveDev && spec.savePeer) },
  ];

  const commands = groups
    .map((group) => ({ ...group, args: group.specs.flatMap((spec) => installArgument(job, spec) ?? []) }))
    .filter((group) => group.args.length > 0)
    .map((group) => [
      "install",
      ...group.args,
      ...(group.flag ? [group.flag] : []),
      ...forceFlag,
      ...job.installArgs.filter((arg) => !group.peer || !saveExactFlags.includes(arg)),
      ...(group.peer ? ["--no-save-exact"] : []),
    ]);
  return [...commands, ...unsavedInstall(job, force)];
}

function uninstallStep(job: MockJob): { commands: string[][]; notes: ReasonPreview[] } {
  const kept = job.packages.filter((spec) => spec.pack === "no-save").map((spec) => spec.name);
  const names = job.packages.filter((spec) => spec.pack !== "no-save").map((spec) => spec.name);
  return {
    commands: names.length > 0 ? [["uninstall", ...names]] : [],
    notes: kept.length > 0 ? [mockReason("packedNotUninstalled", { names: kept.join(", ") })] : [],
  };
}

function runsBeforePacking(job: MockJob, step: StepName): boolean {
  const before = job.steps.indexOf(step);
  return before >= 0 && before < job.steps.indexOf("pack");
}

function packStep(job: MockJob): { commands: string[][]; notes: ReasonPreview[] } {
  const own = dependencyCandidates().find((candidate) => candidate.libraryProject === job.name);
  const destination = own ? libraryFolder(job, own.name) : (job.packFolder ?? "");
  const pack = ["pack", "--json", "--pack-destination", destination];
  const notes = job.steps[0] === "pack" && job.steps.length > 1 ? [mockReason("packedBeforeOwnChanges")] : [];
  if (runsBeforePacking(job, "build")) {
    return { commands: [pack], notes: [...notes, mockReason("builtBeforePacking")] };
  }
  return { commands: mockProject(job.name).hasBuildScript ? [["run", "build"], pack] : [pack], notes };
}

function saveExactPeerNotes(job: MockJob): ReasonPreview[] {
  const hasPeer = job.packages.some((spec) => !spec.saveDev && spec.savePeer);
  const hasExact = job.installArgs.some((arg) => saveExactFlags.includes(arg));
  return hasPeer && hasExact ? [mockReason("saveExactDroppedForPeers")] : [];
}

function previewStep(job: MockJob, step: StepName, version: VersionPreview | null): StepPreview {
  let commands: string[][] = [];
  let notes: ReasonPreview[] = [];

  if (step === "uninstall") ({ commands, notes } = uninstallStep(job));
  if (step === "pack") ({ commands, notes } = packStep(job));
  if (step === "install" || step === "force-install") {
    commands = installCommands(job, step === "force-install");
    notes = saveExactPeerNotes(job);
  }
  if (step === "version" && version) {
    if (version.decision === "bump") commands = [["version", version.level, "--no-git-tag-version"]];
    notes = [versionNote(version)].filter((note): note is ReasonPreview => note !== null);
  }
  if (step === "audit") commands = auditCommands(job);
  if (step === "audit-report") commands = [["audit", "--json"]];
  if (step === "build") {
    if (mockProject(job.name).hasBuildScript) commands = [["run", "build"]];
    else notes = [mockReason("noBuildScript")];
  }

  return { step, commands: commands.map(commandLine), notes };
}

export function previewJob(job: MockJob, plan: RunPlan): JobPreview {
  const version = job.steps.includes("version") ? previewVersion(job, plan) : null;
  return {
    name: job.name,
    dependsOn: [...job.dependsOn],
    steps: job.steps.map((step) => previewStep(job, step, version)),
    version,
  };
}

function noteOnlyPacked(job: JobPreview, withoutPackages: string[]): JobPreview {
  if (!withoutPackages.includes(job.name)) return job;
  const steps = job.steps.map((step) =>
    step.step === "pack" ? { ...step, notes: [...step.notes, mockReason("onlyPacked")] } : step,
  );
  return { ...job, steps };
}

export function mockPreview(plan: RunPlan): PlanPreview {
  const { jobs, missing, withoutPackages } = mockJobs(plan);
  return {
    jobs: jobs.map((job) => noteOnlyPacked(previewJob(job, plan), withoutPackages)),
    missing,
    withoutPackages,
    nothingToDo:
      jobs.length > 0 ? null : mockReason(plan.packages.length === 0 ? "noLibraries" : "noProjectUsesPackages"),
  };
}
