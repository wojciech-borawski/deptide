import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { type App } from "vue";
import { createPinia, setActivePinia, type Pinia } from "pinia";

import type { ProjectGroup, ProjectView, UpdateConfig, WorkspaceSnapshot } from "@/api/types";

const { saveConfig } = vi.hoisted(() => ({ saveConfig: vi.fn() }));
vi.mock("@/api/commands", () => ({ reportError: () => undefined, saveConfig }));
vi.mock("@/components/ui/ModalDialog.vue", async () => {
  const { defineComponent, h } = await import("vue");
  return {
    default: defineComponent({
      props: { open: Boolean, title: { type: String, default: "" } },
      setup:
        (props, { slots }) =>
        () =>
          props.open ? h("div", { role: "dialog" }, [props.title, slots.default?.(), slots.footer?.()]) : null,
    }),
  };
});

import ProjectGroupBar from "@/components/projects/ProjectGroupBar.vue";
import ProjectPickerList from "@/components/projects/ProjectPickerList.vue";
import { i18n } from "@/i18n";
import { useWorkspaceStore } from "@/stores/workspace";
import { makeProjectView } from "./helpers/project-fixtures";
import {
  buttonLabelled,
  click,
  dispatch,
  findAll,
  mount,
  testDocument,
  textOf,
  type Mounted,
  type TestNode,
} from "./helpers/test-renderer";

const available = [makeProjectView("web"), makeProjectView("admin"), makeProjectView("api")];

let pinia: Pinia;
let mounted: Mounted | undefined;

function install(app: App): void {
  app.use(pinia);
  app.use(i18n);
}

function withGroups(groups: ProjectGroup[], projects: ProjectView[] = available): void {
  const snapshot: Partial<WorkspaceSnapshot> = {
    root: "C:\\ws",
    config: { packages: [], projects: projects.map(({ name, path }) => ({ name, path })), groups },
    projects,
  };
  useWorkspaceStore().snapshot = snapshot as WorkspaceSnapshot;
}

function render(selected: string[]): { selections: string[][]; root: Mounted["root"] } {
  const selections: string[][] = [];
  mounted = mount(ProjectGroupBar as never, install as never, {
    available,
    selected,
    onSelect: (names: string[]) => selections.push(names),
  });
  return { selections, root: mounted.root };
}

beforeEach(() => {
  pinia = createPinia();
  setActivePinia(pinia);
  i18n.global.locale.value = "en";
  vi.stubGlobal("document", testDocument);
  saveConfig.mockReset();
  saveConfig.mockImplementation(async (root: string, config: UpdateConfig) => ({
    ...useWorkspaceStore().snapshot,
    root,
    config,
  }));
});

afterEach(() => {
  mounted?.unmount();
  mounted = undefined;
  vi.unstubAllGlobals();
});

describe("project group chips", () => {
  it("a click selects exactly the group, Shift or Ctrl adds it to the selection", () => {
    withGroups([
      { name: "frontend", projects: ["web", "admin", "removed-app"] },
      { name: "services", projects: ["api"] },
    ]);
    const { selections, root } = render(["api"]);
    const frontend = button(root, "frontend");

    dispatch(frontend, "click");
    dispatch(frontend, "click", { shiftKey: true });
    dispatch(frontend, "click", { ctrlKey: true });

    expect(selections).toEqual([
      ["web", "admin"],
      ["api", "web", "admin"],
      ["api", "web", "admin"],
    ]);
  });
});

function button(root: Mounted["root"], label: string): TestNode {
  const found = buttonLabelled(root, label);
  expect(found, `a button labelled ${label}`).toBeDefined();
  return found!;
}

function savedGroups(): ProjectGroup[] | undefined {
  return (saveConfig.mock.lastCall?.[1] as UpdateConfig | undefined)?.groups;
}

function typeName(root: Mounted["root"], name: string): void {
  dispatch(findAll(root, "input")[0]!, "input", { target: { value: name } });
}

describe("saving the selection as a group", () => {
  it("asks for a name and appends a new group with the selected projects", async () => {
    withGroups([{ name: "frontend", projects: ["web"] }]);
    const { root } = render(["web", "api"]);

    click(button(root, "Save selection as group"));
    await vi.waitFor(() => expect(findAll(root, "input")).toHaveLength(1));
    typeName(root, "services");
    await vi.waitFor(() => click(button(root, "Save group")));

    await vi.waitFor(() =>
      expect(savedGroups()).toEqual([
        { name: "frontend", projects: ["web"] },
        { name: "services", projects: ["web", "api"] },
      ]),
    );
  });

  it("asks before replacing a group of the same name", async () => {
    withGroups([{ name: "frontend", projects: ["web"] }]);
    const { root } = render(["admin"]);

    click(button(root, "Save selection as group"));
    await vi.waitFor(() => expect(findAll(root, "input")).toHaveLength(1));
    typeName(root, "frontend");
    await vi.waitFor(() => click(button(root, "Save group")));

    await vi.waitFor(() => expect(textOf(root)).toContain("A group named frontend already exists"));
    expect(saveConfig).not.toHaveBeenCalled();
    click(button(root, "Replace"));
    await vi.waitFor(() => expect(savedGroups()).toEqual([{ name: "frontend", projects: ["admin"] }]));
  });
});

describe("writing groups to disk", () => {
  it("keeps the dialog and the typed name and shows the error when the save fails", async () => {
    withGroups([{ name: "frontend", projects: ["web"] }]);
    saveConfig.mockRejectedValue("disk write denied");
    const { root } = render(["web", "api"]);

    click(button(root, "Save selection as group"));
    await vi.waitFor(() => expect(findAll(root, "input")).toHaveLength(1));
    typeName(root, "services");
    await vi.waitFor(() => click(button(root, "Save group")));

    await vi.waitFor(() => expect(textOf(root)).toContain("disk write denied"));
    expect(findAll(root, "input")[0]!.props.value).toBe("services");
    expect(button(root, "Save group")).toBeDefined();
  });

  it("opens a new dialog without the error of a failed save the user cancelled", async () => {
    withGroups([{ name: "frontend", projects: ["web"] }]);
    saveConfig.mockRejectedValue("disk write denied");
    const { root } = render(["web", "api"]);

    click(button(root, "Save selection as group"));
    await vi.waitFor(() => expect(findAll(root, "input")).toHaveLength(1));
    typeName(root, "services");
    await vi.waitFor(() => click(button(root, "Save group")));
    await vi.waitFor(() => expect(textOf(root)).toContain("disk write denied"));
    click(button(root, "Cancel"));
    await vi.waitFor(() => expect(findAll(root, "input")).toHaveLength(0));
    click(button(root, "Save selection as group"));
    await vi.waitFor(() => expect(findAll(root, "input")).toHaveLength(1));

    expect(textOf(root)).not.toContain("disk write denied");
  });

  it("writes a deletion once while the first write is still pending", async () => {
    withGroups([{ name: "frontend", projects: ["web"] }]);
    saveConfig.mockImplementation(() => new Promise(() => undefined));
    const { root } = render([]);

    click(button(root, "Edit groups"));
    await vi.waitFor(() => click(labelled(root, "Delete frontend")));
    await vi.waitFor(() => expect(textOf(root)).toContain("Delete the group frontend?"));
    click(button(root, "Delete"));
    click(button(root, "Delete"));
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(saveConfig).toHaveBeenCalledTimes(1);
  });

  for (const outcome of ["succeeds", "fails"] as const) {
    it(`leaves a newer dialog alone when a write from a cancelled dialog ${outcome} later`, async () => {
      withGroups([
        { name: "frontend", projects: ["web", "admin"] },
        { name: "services", projects: ["api"] },
      ]);
      let settle: () => void = () => undefined;
      saveConfig.mockImplementationOnce(
        (root: string, config: UpdateConfig) =>
          new Promise((resolve, reject) => {
            settle = () =>
              outcome === "succeeds"
                ? resolve({ ...useWorkspaceStore().snapshot, root, config })
                : reject("late disk error");
          }),
      );
      const { root } = render([]);

      click(button(root, "Edit groups"));
      await vi.waitFor(() => click(labelled(root, "Delete frontend")));
      await vi.waitFor(() => expect(textOf(root)).toContain("Delete the group frontend?"));
      click(button(root, "Delete"));
      click(button(root, "Cancel"));
      await vi.waitFor(() => expect(textOf(root)).not.toContain("Delete the group frontend?"));
      await vi.waitFor(() => click(labelled(root, "Rename services")));
      await vi.waitFor(() => expect(findAll(root, "input")).toHaveLength(1));
      typeName(root, "backend");
      settle();
      await new Promise((resolve) => setTimeout(resolve, 10));

      expect(findAll(root, "input").map((input) => input.props.value)).toEqual(["backend"]);
      expect(textOf(root)).not.toContain("late disk error");
    });
  }
});

function labelled(root: Mounted["root"], label: string): TestNode {
  const found = findAll(root, "button").find((node) => node.props["aria-label"] === label);
  expect(found, `a button with the accessible name ${label}`).toBeDefined();
  return found!;
}

describe("renaming and deleting groups", () => {
  const groups = [
    { name: "frontend", projects: ["web", "admin"] },
    { name: "services", projects: ["api"] },
  ];

  it("renames a group in place and refuses a name another group has", async () => {
    withGroups(groups);
    const { root } = render([]);

    click(button(root, "Edit groups"));
    await vi.waitFor(() => click(labelled(root, "Rename frontend")));
    await vi.waitFor(() => expect(findAll(root, "input")).toHaveLength(1));
    typeName(root, "services");
    await vi.waitFor(() => click(button(root, "Rename")));
    await vi.waitFor(() => expect(textOf(root)).toContain("Another group is already named services."));
    expect(saveConfig).not.toHaveBeenCalled();

    typeName(root, "web apps");
    await vi.waitFor(() => click(button(root, "Rename")));

    await vi.waitFor(() =>
      expect(savedGroups()).toEqual([
        { name: "web apps", projects: ["web", "admin"] },
        { name: "services", projects: ["api"] },
      ]),
    );
  });

  it("deletes a group after a confirmation", async () => {
    withGroups(groups);
    const { root } = render([]);

    click(button(root, "Edit groups"));
    await vi.waitFor(() => click(labelled(root, "Delete frontend")));
    await vi.waitFor(() => expect(textOf(root)).toContain("Delete the group frontend?"));
    expect(saveConfig).not.toHaveBeenCalled();
    click(button(root, "Delete"));

    await vi.waitFor(() => expect(savedGroups()).toEqual([{ name: "services", projects: ["api"] }]));
  });
});

describe("the shared project picker", () => {
  it("offers the group chips above the list and reports a group pick as a selection", async () => {
    withGroups([{ name: "frontend", projects: ["web", "admin"] }]);
    const selections: string[][] = [];
    mounted = mount(ProjectPickerList as never, install as never, {
      projects: available.slice(0, 1),
      available,
      selected: [],
      onSelect: (names: string[]) => selections.push(names),
      onToggle: () => undefined,
    });

    click(button(mounted.root, "frontend"));

    expect(selections).toEqual([["web", "admin"]]);
  });
});
