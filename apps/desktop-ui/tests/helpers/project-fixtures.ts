import type { ProjectView } from "@/api/types";

export function makeProjectView(name: string, overrides: Partial<ProjectView> = {}): ProjectView {
  return {
    name,
    path: `../repos/${name}`,
    absolutePath: `C:\\repos\\${name}`,
    exists: true,
    kind: "application",
    skip: false,
    packages: null,
    installArgs: null,
    duplicateOf: [],
    ...overrides,
  };
}
