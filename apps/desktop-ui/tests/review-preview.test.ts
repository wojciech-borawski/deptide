import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { nextTick, type App } from "vue";
import { createPinia, setActivePinia, type Pinia } from "pinia";

import type { PlanPreview, ReasonPreview, WorkspaceSnapshot } from "@/api/types";

const backend = vi.hoisted(() => ({ previewPlan: vi.fn() }));

vi.mock("@/api/commands", () => ({ ...backend, reportError: () => undefined }));

import StepReview from "@/components/wizard/StepReview.vue";
import { i18n } from "@/i18n";
import { useWizardStore } from "@/stores/wizard";
import { useWorkspaceStore } from "@/stores/workspace";
import { buttonLabelled, click, mount, testDocument, textOf, type Mounted } from "./helpers/test-renderer";

const changedOnBranch: ReasonPreview = {
  kind: "changedOnBranch",
  params: { version: "2.0.1", from: "2.0.0", to: "2.0.1", commit: "a1b2c3d" },
  text: "version 2.0.1 already changed on this branch (2.0.0 -> 2.0.1 in a1b2c3d)",
};

const answer: PlanPreview = {
  jobs: [
    {
      name: "Shop-Frontend",
      dependsOn: [],
      steps: [
        { step: "install", commands: ['npm install "@demo/core@^2.3.0" --no-fund'], notes: [] },
        { step: "version", commands: ["npm version minor --no-git-tag-version"], notes: [] },
        { step: "audit-report", commands: ["npm audit --json"], notes: [] },
      ],
      version: { decision: "bump", current: "1.4.2", next: "1.5.0", level: "minor", reason: null },
    },
    {
      name: "Shop-Admin",
      dependsOn: ["Shop-Frontend"],
      steps: [
        { step: "version", commands: [], notes: [changedOnBranch] },
        {
          step: "build",
          commands: [],
          notes: [{ kind: "noBuildScript", params: {}, text: "no build script, step skipped" }],
        },
      ],
      version: {
        decision: "skip",
        current: "2.0.1",
        level: "minor",
        reason: changedOnBranch,
      },
    },
  ],
  missing: ["Gone-App"],
  withoutPackages: [],
  nothingToDo: null,
};

let pinia: Pinia;
let mounted: Mounted | undefined;

function install(app: App): void {
  app.use(pinia);
  app.use(i18n);
  app.directive("ripple", {});
}

function show(): string {
  mounted = mount(StepReview, install as never);
  return textOf(mounted.root);
}

function text(): string {
  if (!mounted) throw new Error("nothing mounted");
  return textOf(mounted.root);
}

beforeEach(() => {
  backend.previewPlan.mockReset();
  pinia = createPinia();
  setActivePinia(pinia);
  i18n.global.locale.value = "en";
  vi.stubGlobal("document", testDocument);
  useWorkspaceStore().snapshot = { root: "C:\\ws" } as WorkspaceSnapshot;
});

afterEach(() => {
  mounted?.unmount();
  mounted = undefined;
  vi.unstubAllGlobals();
});

describe("Review step preview", () => {
  it("asks the backend for the plan being reviewed and shows loading until it answers", async () => {
    backend.previewPlan.mockReturnValue(new Promise(() => undefined));

    expect(show()).toContain("What will run");
    expect(text()).toContain("Working out the commands…");
    expect(backend.previewPlan).toHaveBeenCalledWith("C:\\ws", useWizardStore().plan);
  });

  it("shows each project's version line and keeps its commands folded until opened", async () => {
    backend.previewPlan.mockResolvedValue(answer);
    show();
    await vi.waitFor(() => expect(text()).toContain("1.4.2 -> 1.5.0 minor"));

    expect(text()).toContain("skipped: version 2.0.1 already changed on this branch (2.0.0 -> 2.0.1 in a1b2c3d)");
    expect(text()).toContain("after Shop-Frontend");
    expect(text()).toContain("3 npm commands");
    expect(text()).toContain("Not in the configuration, skipped: Gone-App");
    expect(text()).not.toContain("npm audit --json");

    const toggle = buttonLabelled(mounted!.root, "Shop-Frontend");
    if (!toggle) throw new Error("no toggle for Shop-Frontend");
    click(toggle);
    await nextTick();
    expect(text()).toContain('npm install "@demo/core@^2.3.0" --no-fund');
    expect(text()).toContain("npm audit --json");
    expect(text()).not.toContain("no build script, step skipped");
  });

  it("shows why the preview failed", async () => {
    backend.previewPlan.mockRejectedValue("Cannot read update-libs.json");
    show();

    await vi.waitFor(() => expect(text()).toContain("Could not work out the commands: Cannot read update-libs.json"));
  });
});

describe("Review step reasons", () => {
  const worded: PlanPreview = {
    jobs: [
      {
        name: "Shop-Admin",
        dependsOn: [],
        steps: [
          {
            step: "version",
            commands: [],
            notes: [
              {
                kind: "changedOnBranch",
                params: { version: "2.0.1", from: "2.0.0", to: "2.0.1", commit: "a1b2c3d" },
                text: "version 2.0.1 already changed on this branch (2.0.0 -> 2.0.1 in a1b2c3d), bump skipped",
              },
            ],
          },
          {
            step: "build",
            commands: [],
            notes: [{ kind: "noBuildScript", params: {}, text: "no build script, step skipped" }],
          },
        ],
        version: {
          decision: "skip",
          current: "2.0.1",
          level: "minor",
          reason: {
            kind: "changedOnBranch",
            params: { version: "2.0.1", from: "2.0.0", to: "2.0.1", commit: "a1b2c3d" },
            text: "version 2.0.1 already changed on this branch (2.0.0 -> 2.0.1 in a1b2c3d)",
          },
        },
      },
      {
        name: "Orders-Api",
        dependsOn: [],
        steps: [],
        version: {
          decision: "bump",
          current: null,
          next: "0.0.1",
          level: "patch",
          reason: null,
        },
      },
    ],
    missing: [],
    withoutPackages: [],
    nothingToDo: null,
  };

  it.each([
    {
      locale: "pl",
      skip: "pominięte: wersja 2.0.1 została już zmieniona na tej gałęzi (2.0.0 -> 2.0.1 w a1b2c3d)",
      note: "wersja 2.0.1 została już zmieniona na tej gałęzi (2.0.0 -> 2.0.1 w a1b2c3d), podbicie pominięte",
      build: "brak skryptu build, krok pominięty",
      bump: "brak -> 0.0.1 patch",
    },
    {
      locale: "de",
      skip: "übersprungen: Version 2.0.1 wurde auf diesem Zweig schon geändert (2.0.0 -> 2.0.1 in a1b2c3d)",
      note: "Version 2.0.1 wurde auf diesem Zweig schon geändert (2.0.0 -> 2.0.1 in a1b2c3d), Erhöhung übersprungen",
      build: "kein Build-Skript, Schritt übersprungen",
      bump: "keine -> 0.0.1 patch",
    },
  ])("words skip reasons and step notes in $locale", async ({ locale, skip, note, build, bump }) => {
    backend.previewPlan.mockResolvedValue(worded);
    i18n.global.locale.value = locale as "pl" | "de";
    show();
    await vi.waitFor(() => expect(text()).toContain("Shop-Admin"));

    const toggle = buttonLabelled(mounted!.root, "Shop-Admin");
    if (!toggle) throw new Error("no toggle for Shop-Admin");
    click(toggle);
    await nextTick();

    expect(text()).toContain(skip);
    expect(text()).toContain(note);
    expect(text()).toContain(build);
    expect(text()).toContain(bump);
    expect(text()).not.toContain("already changed on this branch");
    expect(text()).not.toContain("no build script");
  });

  it.each([
    { locale: "pl", expected: "Nic do zrobienia: żaden z wybranych projektów nie używa wybranych pakietów" },
    { locale: "de", expected: "Nichts zu tun: keines der gewählten Projekte verwendet die gewählten Pakete" },
  ])("words why no project gets a job in $locale", async ({ locale, expected }) => {
    backend.previewPlan.mockResolvedValue({
      jobs: [],
      missing: [],
      withoutPackages: ["Shop-Frontend"],
      nothingToDo: {
        kind: "noProjectUsesPackages",
        params: {},
        text: "Nothing to do: none of the selected projects uses the chosen packages",
      },
    });
    i18n.global.locale.value = locale as "pl" | "de";
    show();

    await vi.waitFor(() => expect(text()).toContain(expected));
    expect(text()).not.toContain("Nothing to do");
  });

  it.each([
    {
      locale: "en",
      expected:
        "skipped: cannot tell whether this branch bumped the version: the folder is not a git repository or git is not installed",
    },
    {
      locale: "pl",
      expected:
        "pominięte: nie da się ustalić, czy ta gałąź podbiła wersję: folder nie jest repozytorium git albo git nie jest zainstalowany",
    },
    {
      locale: "de",
      expected:
        "übersprungen: nicht feststellbar, ob dieser Zweig die Version erhöht hat: der Ordner ist kein Git-Repository oder Git ist nicht installiert",
    },
  ])("words why git could not tell what the branch did in $locale", async ({ locale, expected }) => {
    backend.previewPlan.mockResolvedValue({
      jobs: [
        {
          name: "Shop-Admin",
          dependsOn: [],
          steps: [],
          version: {
            decision: "skip",
            current: "1.0.0",
            level: "minor",
            reason: {
              kind: "branchUnknown",
              params: { cause: "notARepository" },
              text: "cannot tell whether this branch bumped the version: the folder is not a git repository or git is not installed",
            },
          },
        },
      ],
      missing: [],
      withoutPackages: [],
      nothingToDo: null,
    });
    i18n.global.locale.value = locale as "en" | "pl" | "de";
    show();

    await vi.waitFor(() => expect(text()).toContain(expected));
  });
});

describe("Review step preview while it reloads", () => {
  it("marks the previous answer as updating until the new one arrives", async () => {
    let answerNext: (preview: PlanPreview) => void = () => undefined;
    backend.previewPlan
      .mockResolvedValueOnce(answer)
      .mockReturnValueOnce(new Promise<PlanPreview>((resolve) => (answerNext = resolve)));
    show();
    await vi.waitFor(() => expect(text()).toContain("1.4.2 -> 1.5.0 minor"));
    expect(text()).not.toContain("Updating…");

    useWizardStore().draft.steps = ["version"];
    await vi.waitFor(() => expect(backend.previewPlan).toHaveBeenCalledTimes(2));
    await nextTick();

    expect(text()).toContain("Updating…");
    expect(mounted!.root.querySelector('[aria-busy="true"]')).not.toBeNull();
    expect(textOf(mounted!.root.querySelector('[aria-busy="true"]')!)).toContain("Shop-Frontend");

    answerNext({ ...answer, jobs: [] });
    await vi.waitFor(() => expect(text()).not.toContain("Updating…"));
    expect(mounted!.root.querySelector('[aria-busy="true"]')).toBeNull();
  });
});

describe("Review step packed libraries", () => {
  it("marks a packed library and warns when the tarball is saved to package.json", async () => {
    backend.previewPlan.mockReturnValue(new Promise(() => undefined));
    const wizard = useWizardStore();
    wizard.draft.manualPackages = [
      { name: "@demo/core", version: "2.3.0", saveDev: false, savePeer: false, pack: "no-save" },
    ];

    expect(show()).toContain("@demo/core@2.3.0 packed");
    expect(text()).not.toContain("points package.json at the tarball");

    wizard.draft.manualPackages = [
      { name: "@demo/core", version: "2.3.0", saveDev: false, savePeer: false, pack: "save" },
    ];
    await nextTick();
    expect(text()).toContain("points package.json at the tarball");
  });
});
