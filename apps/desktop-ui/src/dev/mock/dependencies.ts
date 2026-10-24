import type { CopyScope, DependencyDrift, DependencyGraph, DriftCell, LibraryDrift, ProjectCopies } from "@/api/types";

function cell(project: string, declared: string, installed: string | null, flags: Partial<DriftCell> = {}): DriftCell {
  return {
    project,
    section: "dependencies",
    declared,
    installed,
    outOfRange: false,
    differs: false,
    behind: false,
    ...flags,
  };
}

const readableProjects = ["Shop-Frontend", "Shop-Admin", "Orders-Api", "Billing-Api", "Demo-Core", "Demo-UiKit"];

function coreDrift(checkLatest: boolean): LibraryDrift {
  const behind = checkLatest;
  return {
    name: "@demo/core",
    localProject: "Demo-Core",
    localVersion: "2.3.0-DEMO-7",
    ambiguousProjects: [],
    newestInstalled: "2.2.1",
    latest: checkLatest ? "2.2.1" : null,
    latestError: null,
    mismatch: true,
    cells: [
      cell("Shop-Frontend", "^2.2.0", "2.2.0", { differs: true, behind }),
      cell("Shop-Admin", "^2.2.0", "2.2.1", { differs: true }),
      cell("Orders-Api", "~2.1.0", "2.2.0", { outOfRange: true, differs: true, behind }),
      cell("Billing-Api", "^2.2.0", null),
      cell("Demo-UiKit", "^2.0.0", "2.2.0", { section: "peer", differs: true, behind }),
    ],
  };
}

function uiKitDrift(checkLatest: boolean): LibraryDrift {
  return {
    name: "@demo/ui-kit",
    localProject: "Demo-UiKit",
    localVersion: "9.0.0-DEMO-3",
    ambiguousProjects: [],
    newestInstalled: "9.0.0-DEMO-1",
    latest: null,
    latestError: checkLatest
      ? "npm error code E404; npm error 404 Not Found - GET https://registry.npmjs.org/@demo%2fui-kit"
      : null,
    mismatch: false,
    cells: [
      cell("Shop-Frontend", "^9.0.0-DEMO-1", "9.0.0-DEMO-1"),
      cell("Shop-Admin", "^9.0.0-DEMO-1", "9.0.0-DEMO-1", { section: "dev" }),
    ],
  };
}

export function mockDependencyDrift(checkLatest: boolean): DependencyDrift {
  return {
    projects: readableProjects,
    unreadable: ["Legacy-Portal"],
    libraries: [coreDrift(checkLatest), uiKitDrift(checkLatest)],
  };
}

function wantsCycle(): boolean {
  return globalThis.location?.search.includes("deps=cycle") ?? false;
}

export function mockDependencyGraph(): DependencyGraph {
  const cycle = wantsCycle();
  return {
    nodes: [
      { name: "Demo-Core", packageName: "@demo/core", layer: 0, dependsOn: [] },
      { name: "Demo-UiKit", packageName: "@demo/ui-kit", layer: 1, dependsOn: [] },
      { name: "Shop-Frontend", packageName: "shop-frontend", layer: 2, dependsOn: ["Demo-Core", "Demo-UiKit"] },
      { name: "Shop-Admin", packageName: "shop-admin", layer: 2, dependsOn: ["Demo-Core", "Demo-UiKit"] },
      { name: "Orders-Api", packageName: "orders-api", layer: 1, dependsOn: ["Demo-Core"] },
      { name: "Billing-Api", packageName: "billing-api", layer: cycle ? 2 : 1, dependsOn: ["Demo-Core"] },
    ],
    edges: [
      { consumer: "Shop-Frontend", library: "Demo-Core", section: "dependencies" },
      { consumer: "Shop-Frontend", library: "Demo-UiKit", section: "dependencies" },
      { consumer: "Shop-Admin", library: "Demo-Core", section: "dependencies" },
      { consumer: "Shop-Admin", library: "Demo-UiKit", section: "dev" },
      { consumer: "Orders-Api", library: "Demo-Core", section: "dependencies" },
      { consumer: "Billing-Api", library: "Demo-Core", section: "dependencies" },
      { consumer: "Demo-UiKit", library: "Demo-Core", section: "peer" },
      ...(cycle
        ? [
            { consumer: "Orders-Api", library: "Billing-Api", section: "dependencies" as const },
            { consumer: "Billing-Api", library: "Orders-Api", section: "dependencies" as const },
          ]
        : []),
    ],
    cycles: cycle ? [["Orders-Api", "Billing-Api"]] : [],
  };
}

export function mockInstalledCopies(scope: CopyScope): ProjectCopies[] {
  const everything = scope === "all-packages";
  return [
    {
      project: "Shop-Frontend",
      hasNodeModules: true,
      packages: [
        {
          name: "@demo/core",
          copies: [
            { version: "2.2.0", path: "node_modules/@demo/core" },
            { version: "2.1.3", path: "node_modules/@demo/ui-kit/node_modules/@demo/core" },
          ],
        },
        ...(everything
          ? [
              {
                name: "lodash",
                copies: [
                  { version: "4.17.21", path: "node_modules/lodash" },
                  { version: "3.10.1", path: "node_modules/legacy-grid/node_modules/lodash" },
                ],
              },
            ]
          : []),
      ],
    },
    { project: "Shop-Admin", hasNodeModules: true, packages: [] },
    {
      project: "Orders-Api",
      hasNodeModules: true,
      packages: everything
        ? [
            {
              name: "tslib",
              copies: [
                { version: "2.8.1", path: "node_modules/tslib" },
                { version: "1.14.1", path: "node_modules/@nestjs/core/node_modules/tslib" },
              ],
            },
          ]
        : [],
    },
    { project: "Billing-Api", hasNodeModules: false, packages: [] },
    { project: "Demo-Core", hasNodeModules: true, packages: [] },
    { project: "Demo-UiKit", hasNodeModules: true, packages: [] },
  ];
}
