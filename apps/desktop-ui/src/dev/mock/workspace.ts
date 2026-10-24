import type {
  DependencyCandidate,
  ProjectView,
  RunBackup,
  RunSummary,
  ScannedProject,
  Settings,
  UpdateConfig,
  WorkspaceSnapshot,
} from "@/api/types";

export const fakeRoot = "C:\\demo\\deptide-workspace";

export const projectsRoot = "C:\\demo\\repos";

export const workspaceState: {
  settings: Settings;
  config: UpdateConfig;
  history: RunSummary[];
  backups: RunBackup[];
} = {
  settings: {
    projectsRoot,
    scanDepth: 4,
    concurrency: 3,
    steps: ["uninstall", "install", "audit", "build"],
    mode: "per-project",
    extraIgnoredDirectories: [],
    branchSuffixPattern: "^([A-Za-z]+-\\d+)",
  },
  config: {
    installArgs: ["--no-fund"],
    packages: [{ name: "@demo/core", version: "2.2.0", saveDev: false, savePeer: false }],
    projects: [
      { name: "Shop-Frontend", path: "../repos/shop/frontend" },
      { name: "Shop-Admin", path: "../repos/shop/admin" },
      { name: "Orders-Api", path: "../repos/services/orders-api" },
      { name: "Billing-Api", path: "../repos/services/billing-api" },
      { name: "Legacy-Portal", path: "../repos/legacy/portal" },
      { name: "Demo-Core", path: "../repos/libs/core" },
      { name: "Demo-UiKit", path: "../repos/libs/ui-kit" },
    ],
    groups: [
      { name: "Shop", projects: ["Shop-Frontend", "Shop-Admin"] },
      { name: "Services", projects: ["Orders-Api", "Billing-Api"] },
    ],
    transferIgnore: ["*.log", "coverage/", ".env.local"],
  },
  history: [
    pastRun("2026-10-20T08-15-00", "core 2.2.0", ["Shop-Frontend", "Shop-Admin", "Orders-Api"], { backedUp: true }),
    pastRun("2026-10-12T14-02-00", "core 2.1.4", ["Shop-Frontend", "Billing-Api"], { backedUp: false }),
    pastRun("2026-10-11T09-40-00", "core 2.1.4", ["Shop-Frontend"], { dryRun: true }),
  ],
  backups: [
    {
      run: "2026-10-20T08-15-00-core-2-2-0",
      logFile: `${fakeRoot}\\logs\\2026-10-20T08-15-00-core-2-2-0.log`,
      folder: "2026-10-20T08-15-00-core-2-2-0",
      projects: ["Shop-Frontend", "Shop-Admin", "Orders-Api"].map((name) => ({
        name,
        directory: `${projectsRoot}\\${name}`,
        files: name === "Orders-Api" ? ["package.json"] : ["package.json", "package-lock.json"],
      })),
    },
  ],
};

function pastRun(
  stamp: string,
  label: string,
  projectNames: string[],
  options: { backedUp?: boolean; dryRun?: boolean },
): RunSummary {
  const run = `${stamp}-${label.replace(/[^a-z0-9]+/g, "-")}`;
  const startedAt = `${stamp.slice(0, 10)}T${stamp.slice(11).replace(/-/g, ":")}.000Z`;
  return {
    label,
    packages: [`@demo/core@${label.split(" ")[1] ?? ""}`],
    projectCount: projectNames.length,
    concurrency: 3,
    steps: ["install", "build"],
    mode: "per-project",
    dryRun: Boolean(options.dryRun),
    startedAt,
    finishedAt: startedAt,
    totalDurationMs: 42_000,
    busyDurationMs: 61_000,
    logFile: `${fakeRoot}\\logs\\${run}.log`,
    projects: projectNames.map((name) => ({
      name,
      directory: `${projectsRoot}\\${name}`,
      status: "ok",
      durationMs: 20_000,
      stepTimings: [],
      error: "",
      warning: "",
      installed: [],
      diagnosis: null,
      dependencyChanges: [],
      retries: 0,
    })),
    okCount: projectNames.length,
    warnCount: 0,
    failedCount: 0,
    skippedCount: 0,
    aborted: false,
    ...(options.backedUp ? { backupFolder: run } : {}),
  };
}

export function withConfiguredGroups(config: UpdateConfig): UpdateConfig {
  const configured = new Set(config.projects.map((project) => project.name));
  const groups = (config.groups ?? [])
    .map((group) => ({ ...group, projects: group.projects.filter((name) => configured.has(name)) }))
    .filter((group) => group.projects.length > 0);
  return { ...config, groups };
}

export function runFileName(startedAtMs: number, label: string): string {
  const stamp = new Date(startedAtMs).toISOString().slice(0, 19).replace(/:/g, "-");
  const slug = label
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");
  return slug ? `${stamp}-${slug}` : stamp;
}

export function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export function projectViews(): ProjectView[] {
  return workspaceState.config.projects.map((project) => ({
    name: project.name,
    path: project.path,
    absolutePath: `${projectsRoot}\\${project.path.replace("../repos/", "").replace(/\//g, "\\")}`,
    exists: !project.path.includes("legacy"),
    kind: project.path.includes("libs") ? "library" : "application",
    skip: Boolean(project.skip),
    packages: project.packages ?? null,
    installArgs: project.installArgs ?? null,
    duplicateOf: [],
  }));
}

export function workspaceSnapshot(): WorkspaceSnapshot {
  return {
    root: fakeRoot,
    configFile: `${fakeRoot}\\update-libs.json`,
    config: workspaceState.config,
    projects: projectViews(),
    settings: workspaceState.settings,
    savedRuns: [],
    history: [...workspaceState.history],
    recent: [{ path: fakeRoot, lastOpened: new Date().toISOString() }],
  };
}

export function dependencyCandidates(): DependencyCandidate[] {
  const consumers = workspaceState.config.projects
    .filter((project) => !project.path.includes("legacy"))
    .map((project) => project.name);

  return [
    {
      name: "@demo/core",
      localVersion: "2.3.0-DEMO-7",
      localBranch: "ABC-123-new-widgets",
      branchSuffix: "ABC-123",
      currentRanges: ["2.2.0"],
      usedBy: consumers,
      isDevDependency: false,
      sections: ["dependencies"],
      libraryProject: "Demo-Core",
      ambiguousProjects: [],
    },
    {
      name: "@demo/ui-kit",
      localVersion: "9.0.0-DEMO-3",
      localBranch: "main",
      branchSuffix: null,
      currentRanges: ["9.0.0-DEMO-1"],
      usedBy: consumers.slice(0, 2),
      isDevDependency: false,
      sections: ["peer"],
      libraryProject: "Demo-UiKit",
      ambiguousProjects: [],
    },
    {
      name: "vue",
      localVersion: null,
      localBranch: null,
      branchSuffix: null,
      currentRanges: ["3.5.0"],
      usedBy: consumers,
      isDevDependency: true,
      sections: ["peer", "dev"],
      libraryProject: null,
      ambiguousProjects: [],
    },
    {
      name: "typescript",
      localVersion: null,
      localBranch: null,
      branchSuffix: null,
      currentRanges: ["5.9.0"],
      usedBy: consumers,
      isDevDependency: true,
      sections: ["dev"],
      libraryProject: null,
      ambiguousProjects: [],
    },
  ];
}

export function scannedProjects(): ScannedProject[] {
  const known = new Map(workspaceState.config.projects.map((project) => [project.path, project.name]));
  const paths = [
    "shop/frontend",
    "shop/admin",
    "services/orders-api",
    "services/billing-api",
    "services/payments-api",
    "libs/core",
  ];

  return paths.map((relative) => {
    const configPath = `../repos/${relative}`;
    const isLibrary = relative.startsWith("libs");
    const dependencies: Record<string, string> = isLibrary ? {} : { "@demo/core": "2.2.0", vue: "^3.5.0" };
    return {
      directory: `${projectsRoot}\\${relative.replace(/\//g, "\\")}`,
      relativePath: relative,
      proposedName: relative
        .split("/")
        .map((segment) => segment.replace(/(^|-)(\w)/g, (match) => match.toUpperCase()))
        .join("-"),
      packageName: isLibrary ? "@demo/core" : null,
      version: isLibrary ? "2.3.0-DEMO-7" : "1.0.0",
      branch: isLibrary ? "ABC-123-new-widgets" : null,
      kind: isLibrary ? "library" : "application",
      dependencies,
      devDependencies: {},
      peerDependencies: {},
      hasBuildScript: true,
      configPath,
      configuredName: known.get(configPath) ?? null,
    };
  });
}
