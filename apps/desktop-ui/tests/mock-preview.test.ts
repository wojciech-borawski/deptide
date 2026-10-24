import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { BumpWhen, PlanPreview, RunBackup, RunPlan, StepName } from "@/api/types";
import { handleMockCommand } from "@/dev/mock/handlers";
import { mockPreview } from "@/dev/mock/preview";
import { MockRun } from "@/dev/mock/run";
import { workspaceState } from "@/dev/mock/workspace";

function plan(when: BumpWhen, steps: StepName[], withLibrary = true): RunPlan {
  return {
    projectNames: ["Shop-Frontend", "Shop-Admin", "Orders-Api", "Billing-Api"],
    packages: withLibrary
      ? [
          { name: "@demo/core", version: "2.3.0-DEMO-7", saveDev: false, savePeer: false },
          { name: "@demo/ui-kit", version: "^9.0.0", saveDev: false, savePeer: false },
        ]
      : [],
    steps,
    mode: "per-project",
    concurrency: 2,
    dryRun: false,
    extraInstallArgs: ["--cache=C:\\npm cache"],
    label: "parity",
    saveAs: null,
    version: { bump: "minor", when },
    projectBumps: { "Orders-Api": "major" },
  };
}

async function runToEnd(run: MockRun): Promise<void> {
  const done = run.execute();
  await vi.runAllTimersAsync();
  await done;
}

describe("mock preview", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it.each<BumpWhen>(["always", "same-as-main", "not-bumped-on-branch"])(
    "lists the npm lines and version notes the mock run logs (%s)",
    async (when) => {
      const runPlan = plan(when, ["uninstall", "install", "version", "audit", "audit-report", "build"]);
      const preview = mockPreview(runPlan);
      const run = new MockRun(runPlan);
      await runToEnd(run);

      for (const job of preview.jobs) {
        const logged = run.state.jobs.find((entry) => entry.name === job.name)?.log ?? [];
        const expected = job.steps.flatMap((step) => [
          ...step.notes.map((note) => note.text),
          ...step.commands.map((line) => `$ ${line}`),
        ]);
        const announced = logged.filter((line) => line.startsWith("$ ") || expected.includes(line));
        expect(announced, job.name).toEqual(expected);
      }
    },
  );

  it("records the audit counts the preview's projects report", async () => {
    const runPlan = plan("always", ["audit-report"], false);
    const run = new MockRun(runPlan);
    await runToEnd(run);

    expect(run.state.jobs.map((job) => job.stepTimings.map((timing) => timing.step))).toEqual([
      ["audit-report"],
      ["audit-report"],
      ["audit-report"],
      ["audit-report"],
    ]);
    const billing = run.state.jobs.find((job) => job.name === "Billing-Api");
    expect(billing?.audit).toEqual({ info: 0, low: 0, moderate: 1, high: 2, critical: 1, total: 4 });
    expect(run.state.summary?.projects.find((project) => project.name === "Billing-Api")?.audit?.total).toBe(4);
  });
});

describe("mock terminal run", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("runs the command in every selected project, though a terminal plan has no npm step", async () => {
    const terminalPlan: RunPlan = { ...plan("always", [], false), projectNames: ["Shop-Frontend", "Orders-Api"] };
    const run = new MockRun(terminalPlan, "git status");
    await runToEnd(run);

    expect(run.state.jobs.map((job) => job.name)).toEqual(["Shop-Frontend", "Orders-Api"]);
    expect(run.state.jobs.map((job) => job.log[0])).toEqual(["$ git status", "$ git status"]);
  });
});

describe("mock preview follows the engine's job policy", () => {
  const libraryPlan = (projectNames: string[], packages: RunPlan["packages"]): RunPlan => ({
    ...plan("always", ["install"]),
    projectNames,
    packages,
    extraInstallArgs: ["--legacy-peer-deps"],
  });
  const leftPad = { name: "left-pad", version: "1.3.0", saveDev: false, savePeer: false };
  let savedProjects: string;

  beforeEach(() => {
    savedProjects = JSON.stringify(workspaceState.config.projects);
  });

  afterEach(() => {
    workspaceState.config.projects = JSON.parse(savedProjects) as typeof workspaceState.config.projects;
    vi.useRealTimers();
  });

  function installLines(preview: PlanPreview): Record<string, string[]> {
    return Object.fromEntries(preview.jobs.map((job) => [job.name, job.steps.flatMap((step) => step.commands)]));
  }

  it("appends the run's extra arguments to a project's own install arguments", () => {
    const shop = workspaceState.config.projects.find((project) => project.name === "Shop-Frontend");
    if (!shop) throw new Error("no Shop-Frontend in the mock workspace");
    shop.installArgs = ["--no-audit"];

    const preview = mockPreview(libraryPlan(["Shop-Frontend", "Orders-Api"], [leftPad]));

    expect(installLines(preview)).toEqual({
      "Shop-Frontend": ["npm install left-pad@1.3.0 --no-audit --legacy-peer-deps"],
      "Orders-Api": ["npm install left-pad@1.3.0 --no-fund --legacy-peer-deps"],
    });
  });

  it("gives a library only to projects that declare it, unless none does, and honours project restrictions", () => {
    const uiKit = { name: "@demo/ui-kit", version: "9.0.0", saveDev: false, savePeer: false };
    const core = { name: "@demo/core", version: "2.3.0", saveDev: false, savePeer: false };
    const admin = workspaceState.config.projects.find((project) => project.name === "Shop-Admin");
    if (!admin) throw new Error("no Shop-Admin in the mock workspace");
    admin.packages = ["@demo/core"];

    const declared = mockPreview(libraryPlan(["Shop-Frontend", "Orders-Api"], [uiKit]));
    expect(installLines(declared)).toEqual({
      "Shop-Frontend": ["npm install @demo/ui-kit@9.0.0 --save-peer --no-fund --legacy-peer-deps --no-save-exact"],
    });
    expect(declared.withoutPackages).toEqual(["Orders-Api"]);

    const undeclared = mockPreview(libraryPlan(["Shop-Frontend", "Orders-Api"], [leftPad]));
    expect(Object.keys(installLines(undeclared))).toEqual(["Shop-Frontend", "Orders-Api"]);

    const restricted = mockPreview(libraryPlan(["Shop-Frontend", "Shop-Admin"], [core, uiKit]));
    expect(installLines(restricted)).toEqual({
      "Shop-Frontend": [
        "npm install @demo/core@2.3.0 --no-fund --legacy-peer-deps",
        "npm install @demo/ui-kit@9.0.0 --save-peer --no-fund --legacy-peer-deps --no-save-exact",
      ],
      "Shop-Admin": ["npm install @demo/core@2.3.0 --no-fund --legacy-peer-deps"],
    });
  });

  it("gives a project whose package.json cannot be read every library it allows, as the engine does", () => {
    const uiKit = { name: "@demo/ui-kit", version: "9.0.0", saveDev: false, savePeer: true };

    const preview = mockPreview(libraryPlan(["Shop-Frontend", "Legacy-Portal"], [uiKit]));

    expect(preview.withoutPackages).toEqual([]);
    expect(installLines(preview)["Legacy-Portal"]).toEqual([
      "npm install @demo/ui-kit@9.0.0 --save-peer --no-fund --legacy-peer-deps --no-save-exact",
    ]);
  });

  it("runs and reports only the libraries each project's job installs", async () => {
    vi.useFakeTimers();
    const core = { name: "@demo/core", version: "2.3.0", saveDev: false, savePeer: false };
    const uiKit = { name: "@demo/ui-kit", version: "9.0.0", saveDev: false, savePeer: false };
    const admin = workspaceState.config.projects.find((project) => project.name === "Shop-Admin");
    if (!admin) throw new Error("no Shop-Admin in the mock workspace");
    admin.packages = ["@demo/core"];

    const run = new MockRun(libraryPlan(["Shop-Admin"], [core, uiKit]));
    await runToEnd(run);

    const [job] = run.state.jobs;
    expect(job?.packages).toEqual(["@demo/core@2.3.0"]);
    expect(job?.installed.map((entry) => entry.name)).toEqual(["@demo/core"]);
  });

  it("says why --save-exact is left off a peer install, in the preview and the run log", async () => {
    vi.useFakeTimers();
    const uiKit = { name: "@demo/ui-kit", version: "9.0.0", saveDev: false, savePeer: false };
    const peerPlan: RunPlan = {
      ...libraryPlan(["Shop-Frontend"], [uiKit]),
      steps: ["install", "force-install"],
      extraInstallArgs: ["--save-exact"],
    };
    const note = "--save-exact dropped for peer installs: an exact peer range pins consumers to one version";

    const steps = mockPreview(peerPlan).jobs[0]?.steps ?? [];
    expect(steps.map((step) => [step.step, step.notes.map((entry) => [entry.kind, entry.text])])).toEqual([
      ["install", [["saveExactDroppedForPeers", note]]],
      ["force-install", [["saveExactDroppedForPeers", note]]],
    ]);

    const run = new MockRun(peerPlan);
    await runToEnd(run);
    expect(run.state.jobs[0]?.log.filter((line) => line === note || line.startsWith("$ "))).toEqual([
      note,
      "$ npm install @demo/ui-kit@9.0.0 --save-peer --no-fund --no-save-exact",
      note,
      "$ npm install @demo/ui-kit@9.0.0 --save-peer --force --no-fund --no-save-exact",
    ]);
  });

  it("keeps the plan's step order, as the engine does", () => {
    const preview = mockPreview({ ...plan("always", ["build", "version"], false), projectNames: ["Shop-Frontend"] });

    expect(preview.jobs[0]?.steps.map((step) => step.step)).toEqual(["build", "version"]);
  });

  it.each(["constructor", "__proto__", "toString"])("previews the version of a project named %s", (name) => {
    workspaceState.config.projects.push({ name, path: "../repos/custom" });
    const versionPlan: RunPlan = {
      ...plan("always", ["version"], false),
      projectNames: [name],
      version: { bump: "patch", when: "always" },
      projectBumps: {},
    };

    const [job] = mockPreview(versionPlan).jobs;

    expect(job?.version).toEqual({ decision: "bump", current: "1.0.0", next: "1.0.1", level: "patch", reason: null });
    expect(job?.steps.flatMap((step) => step.commands)).toEqual(["npm version patch --no-git-tag-version"]);
  });
});

describe("mock preview of a packed library", () => {
  const packed = {
    name: "@demo/core",
    version: "2.3.0-DEMO-7",
    saveDev: false,
    savePeer: false,
    pack: "no-save" as const,
  };
  const packPlan: RunPlan = {
    ...plan("always", ["uninstall", "install"]),
    projectNames: ["Shop-Frontend"],
    packages: [packed],
    extraInstallArgs: [],
    label: "pack core",
  };
  const folder = "C:\\demo\\deptide-workspace\\packs\\2026-10-22T10-00-00-pack-core\\demo+core-a2c8dc8a";

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-10-22T10:00:00Z"));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("packs the library in its own job first and installs the tarball without saving", () => {
    const preview = mockPreview(packPlan);

    expect(
      preview.jobs.map((job) => ({
        name: job.name,
        dependsOn: job.dependsOn,
        steps: job.steps.map((step) => [step.step, step.commands, step.notes.map((note) => note.text)]),
      })),
    ).toEqual([
      {
        name: "Demo-Core",
        dependsOn: [],
        steps: [["pack", ["npm run build", `npm pack --json --pack-destination ${folder}`], []]],
      },
      {
        name: "Shop-Frontend",
        dependsOn: ["Demo-Core"],
        steps: [
          ["uninstall", [], ["@demo/core not uninstalled: installed from a packed tarball without saving"]],
          ["install", [`npm install ${folder}\\demo-core-2.3.0-DEMO-7.tgz --no-fund --no-save`], []],
        ],
      },
    ]);
  });

  it("installs the tarball again after audit fix, as the engine does", () => {
    const preview = mockPreview({ ...packPlan, steps: ["uninstall", "install", "audit"] });

    const audit = preview.jobs.find((job) => job.name === "Shop-Frontend")?.steps.find((step) => step.step === "audit");
    expect(audit?.commands).toEqual([
      "npm audit fix",
      `npm install ${folder}\\demo-core-2.3.0-DEMO-7.tgz --no-fund --no-save`,
    ]);
  });

  it("lists a selected library that gets none of the packages and only packs it, never into itself", () => {
    const preview = mockPreview({ ...packPlan, projectNames: ["Demo-Core", "Shop-Frontend"] });

    const core = preview.jobs.find((job) => job.name === "Demo-Core");
    expect(preview.withoutPackages).toEqual(["Demo-Core"]);
    expect(core?.steps.map((step) => [step.step, step.notes.map((note) => note.text)])).toEqual([
      ["pack", ["only packed: none of the chosen libraries applies to this project, so its other steps do not run"]],
    ]);
  });

  const vue = { name: "vue", version: "^3.5.0", saveDev: false, savePeer: false };

  it("says in per-step mode that a library is packed before its own install phase", () => {
    const preview = mockPreview({
      ...packPlan,
      mode: "per-step",
      projectNames: ["Demo-Core", "Shop-Frontend"],
      packages: [packed, vue],
    });

    const core = preview.jobs.find((job) => job.name === "Demo-Core");
    expect(core?.steps.map((step) => [step.step, step.notes.map((note) => note.text)])[0]).toEqual([
      "pack",
      ["packed before its own install phase, so the tarball does not hold this run's changes to it"],
    ]);
  });

  it("names the tarball of a library the run bumps before packing", () => {
    const preview = mockPreview({
      ...packPlan,
      steps: ["version", "uninstall", "install"],
      projectNames: ["Demo-Core", "Shop-Frontend"],
      packages: [packed, vue],
    });

    const frontend = preview.jobs.find((job) => job.name === "Shop-Frontend");
    const core = preview.jobs.find((job) => job.name === "Demo-Core");
    expect(core?.version?.decision === "bump" && core.version.next).toBe("2.3.0");
    expect(frontend?.steps.find((step) => step.step === "install")?.commands.at(-1)).toBe(
      `npm install ${folder}\\demo-core-2.3.0.tgz --no-fund --no-save`,
    );
    expect(frontend?.dependsOn).toEqual(["Demo-Core"]);
  });

  it("starts a consumer only after the pack job finished", async () => {
    const run = new MockRun(packPlan);
    await runToEnd(run);

    const [core, frontend] = run.state.jobs;
    expect(frontend?.dependsOn).toEqual(["Demo-Core"]);
    expect(frontend?.startedAtMs ?? 0).toBeGreaterThanOrEqual((core?.startedAtMs ?? 0) + (core?.durationMs ?? 0));
  });

  it("reports the packed version as installed when the run bumps the library before packing", async () => {
    const run = new MockRun({
      ...packPlan,
      steps: ["install", "version", "build"],
      projectNames: ["Demo-Core", "Shop-Frontend"],
      packages: [packed, vue],
    });
    await runToEnd(run);

    const frontend = run.state.jobs.find((job) => job.name === "Shop-Frontend");
    expect(frontend?.installed.find((entry) => entry.name === "@demo/core")).toMatchObject({
      expected: "2.3.0",
      installed: "2.3.0",
      matches: true,
    });
  });

  it("backs up the consumer but not a job that only packs, as the engine does", async () => {
    const run = new MockRun(packPlan);
    await runToEnd(run);

    const backup = workspaceState.backups.find((entry) => entry.run === run.state.summary?.backupFolder);
    expect(backup?.projects.map((project) => project.name)).toEqual(["Shop-Frontend"]);
    expect(run.state.jobs.map((job) => [job.name, job.backupAvailable])).toEqual([
      ["Demo-Core", false],
      ["Shop-Frontend", true],
    ]);
  });

  it("lists no backup of a run whose only job packs, as the engine does", async () => {
    const run = new MockRun({ ...packPlan, projectNames: ["Demo-Core"], label: "only pack" });
    await runToEnd(run);

    const backups = (await handleMockCommand("list_run_backups", {})) as RunBackup[];
    expect(run.state.jobs.map((job) => [job.name, job.status, job.backupAvailable])).toEqual([
      ["Demo-Core", "ok", false],
    ]);
    expect(backups.filter((backup) => backup.logFile === run.state.summary?.logFile)).toEqual([]);
  });

  it("lists the pack job before its consumer and finishes them one at a time", async () => {
    const run = new MockRun({ ...packPlan, concurrency: 1 });
    await runToEnd(run);

    expect(run.state.jobs.map((job) => [job.name, job.status])).toEqual([
      ["Demo-Core", "ok"],
      ["Shop-Frontend", "ok"],
    ]);
  });
});
