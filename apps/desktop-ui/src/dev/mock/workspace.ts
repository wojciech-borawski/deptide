import type {
  DependencyCandidate,
  ProjectView,
  RunSummary,
  ScannedProject,
  Settings,
  UpdateConfig,
  WorkspaceSnapshot,
} from "@/api/types";

export const fakeRoot = "C:\\demo\\deptide-workspace";

export const projectsRoot = "C:\\demo\\repos";

export const workspaceState: { settings: Settings; config: UpdateConfig; history: RunSummary[] } = {
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
    packages: [{ name: "@demo/core", version: "2.2.0", saveDev: false }],
    projects: [
      { name: "Shop-Frontend", path: "../repos/shop/frontend" },
      { name: "Shop-Admin", path: "../repos/shop/admin" },
      { name: "Orders-Api", path: "../repos/services/orders-api" },
      { name: "Billing-Api", path: "../repos/services/billing-api" },
      { name: "Legacy-Portal", path: "../repos/legacy/portal" },
    ],
  },
  history: [],
};

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
    },
    {
      name: "@demo/ui-kit",
      localVersion: "9.0.0-DEMO-3",
      localBranch: "main",
      branchSuffix: null,
      currentRanges: ["9.0.0-DEMO-1"],
      usedBy: consumers.slice(0, 2),
      isDevDependency: false,
    },
    {
      name: "vue",
      localVersion: null,
      localBranch: null,
      branchSuffix: null,
      currentRanges: ["3.5.0"],
      usedBy: consumers,
      isDevDependency: false,
    },
    {
      name: "typescript",
      localVersion: null,
      localBranch: null,
      branchSuffix: null,
      currentRanges: ["5.9.0"],
      usedBy: consumers,
      isDevDependency: true,
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
      hasBuildScript: true,
      configPath,
      configuredName: known.get(configPath) ?? null,
    };
  });
}
