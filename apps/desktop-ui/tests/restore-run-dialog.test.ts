import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { h, nextTick, reactive, type App } from "vue";
import { createPinia, setActivePinia, type Pinia } from "pinia";

import type { ProjectRestore, RunBackup, WorkspaceSnapshot } from "@/api/types";

const backend = vi.hoisted(() => ({
  listRunBackups: vi.fn(),
  restoreRunFiles: vi.fn(),
  startCommandRun: vi.fn(),
  push: vi.fn(),
}));

vi.mock("@/api/commands", () => ({
  listRunBackups: backend.listRunBackups,
  restoreRunFiles: backend.restoreRunFiles,
  startCommandRun: backend.startCommandRun,
  reportError: () => undefined,
}));
vi.mock("@/api/events", () => ({ listenRunEvents: async () => () => undefined }));
vi.mock("vue-router", () => ({ useRouter: () => ({ push: backend.push }) }));
vi.mock("@/router", () => ({ routeNames: { terminal: "terminal" } }));
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

import RestoreRunDialog from "@/components/run/RestoreRunDialog.vue";
import { i18n } from "@/i18n";
import { useTerminalStore } from "@/stores/terminal";
import { useWorkspaceStore } from "@/stores/workspace";
import {
  buttonLabelled,
  click,
  mount,
  testDocument,
  textOf,
  type Mounted,
  type TestNode,
} from "./helpers/test-renderer";

const backup: RunBackup = {
  run: "2026-10-22T09-30-00-acme-core",
  logFile: "C:\\ws\\logs\\2026-10-22T09-30-00-acme-core.log",
  folder: "2026-10-22T09-30-00-acme-core",
  projects: [
    { name: "web", directory: "C:\\repos\\web", files: ["package.json", "package-lock.json"] },
    { name: "api", directory: "C:\\repos\\api", files: ["package.json"] },
  ],
};

const results: ProjectRestore[] = [
  { name: "web", outcome: "restored", files: ["package.json", "package-lock.json"] },
  {
    name: "api",
    outcome: "failed",
    error: 'Not restored: the run "nightly" in deptide-cli is working in this project',
  },
];

let pinia: Pinia;
let mounted: Mounted | undefined;

function install(app: App): void {
  app.use(pinia);
  app.use(i18n);
  app.directive("ripple", {});
}

function button(root: TestNode, label: string): TestNode {
  const found = buttonLabelled(root, label);
  expect(found, `a button labelled ${label}`).toBeDefined();
  return found!;
}

function render(): TestNode {
  mounted = mount(RestoreRunDialog as never, install as never, {
    open: true,
    backup,
    label: "acme core",
    onClose: () => undefined,
  });
  return mounted.root;
}

beforeEach(() => {
  pinia = createPinia();
  setActivePinia(pinia);
  i18n.global.locale.value = "en";
  vi.stubGlobal("document", testDocument);
  for (const fn of Object.values(backend)) fn.mockReset();
  backend.restoreRunFiles.mockResolvedValue(results);
  useWorkspaceStore().snapshot = { root: "C:\\ws" } as WorkspaceSnapshot;
});

afterEach(() => {
  mounted?.unmount();
  mounted = undefined;
  vi.unstubAllGlobals();
});

describe("restoring a whole run", () => {
  it("lists every backed-up project with its files, restores them on confirm and reports each project", async () => {
    const root = render();

    expect(textOf(root)).toContain("web: package.json, package-lock.json");
    expect(textOf(root)).toContain("api: package.json");
    expect(backend.restoreRunFiles).not.toHaveBeenCalled();

    click(button(root, "Restore 2 projects"));

    await vi.waitFor(() => expect(textOf(root)).toContain("web: restored package.json, package-lock.json"));
    expect(backend.restoreRunFiles).toHaveBeenCalledWith("C:\\ws", backup.run, ["web", "api"]);
    expect(textOf(root)).toContain(
      'api: failed: Not restored: the run "nightly" in deptide-cli is working in this project',
    );
    expect(textOf(root)).toContain("Run npm install in the restored projects");
  });

  it("opens the Terminal with npm install and only the restored projects selected, without starting it", async () => {
    const root = render();
    click(button(root, "Restore 2 projects"));

    await vi.waitFor(() => click(button(root, "Open Terminal with npm install")));

    const terminal = useTerminalStore();
    expect(terminal.projectNames).toEqual(["web"]);
    expect(terminal.command).toBe("npm install");
    expect(backend.push).toHaveBeenCalledWith({ name: "terminal" });
    expect(backend.startCommandRun).not.toHaveBeenCalled();
  });

  it("drops the answer of a restore confirmed before the dialog was closed and opened for another run", async () => {
    let answerFirst!: (results: ProjectRestore[]) => void;
    backend.restoreRunFiles.mockImplementationOnce(() => new Promise((resolve) => (answerFirst = resolve)));
    const other: RunBackup = { ...backup, run: "other", projects: [backup.projects[1]!] };
    const shown = reactive({ open: true, backup, label: "acme core" });
    const host = {
      setup: () => () => h(RestoreRunDialog, { ...shown, onClose: () => (shown.open = false) }),
    };
    mounted = mount(host, install as never);
    const root = mounted.root;

    click(button(root, "Restore 2 projects"));
    click(button(root, "Cancel"));
    await nextTick();
    Object.assign(shown, { open: true, backup: other, label: "other run" });
    await nextTick();
    answerFirst(results);
    await new Promise((resolve) => setTimeout(resolve, 0));
    await nextTick();

    expect(textOf(root)).not.toContain("web: restored");
    expect(textOf(root)).toContain("api: package.json");
    click(button(root, "Restore 1 project"));
    await vi.waitFor(() => expect(backend.restoreRunFiles).toHaveBeenLastCalledWith("C:\\ws", "other", ["api"]));
  });
});
