import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { h, nextTick, reactive, ref, type App } from "vue";
import { createPinia, setActivePinia, type Pinia } from "pinia";

vi.mock("@/api/commands", () => ({ reportError: () => undefined }));
vi.mock("@/api/events", () => ({ listenRunEvents: async () => () => undefined }));
vi.mock("vue-router", () => ({ useRouter: () => ({ push: vi.fn() }) }));
vi.mock("@/router", () => ({ routeNames: { run: "run" } }));
for (const step of ["StepProjects", "StepLibraries", "StepOptions", "StepReview"]) {
  vi.doMock(`@/components/wizard/${step}.vue`, () => ({ default: { render: () => null } }));
}

import ModalDialog from "@/components/ui/ModalDialog.vue";
import { useModal } from "@/composables/useModal";
import { i18n } from "@/i18n";
import { useWizardStore } from "@/stores/wizard";
import WizardView from "@/views/WizardView.vue";
import { buttonLabelled, findAll, focused, mount, testDocument, textOf, type Mounted } from "./helpers/test-renderer";

let pinia: Pinia;
let mounted: Mounted[] = [];
let keyHandlers: ((event: KeyboardEvent) => void)[] = [];

const fakeWindow = {
  addEventListener: (type: string, handler: (event: KeyboardEvent) => void) => {
    if (type === "keydown") keyHandlers.push(handler);
  },
  removeEventListener: (type: string, handler: (event: KeyboardEvent) => void) => {
    if (type === "keydown") keyHandlers = keyHandlers.filter((entry) => entry !== handler);
  },
};

function press(key: string, modifiers: Partial<KeyboardEvent> = {}): void {
  const event = {
    key,
    altKey: false,
    shiftKey: false,
    ctrlKey: false,
    metaKey: false,
    target: null,
    preventDefault() {},
    ...modifiers,
  };
  for (const handler of [...keyHandlers]) handler(event as unknown as KeyboardEvent);
}

function install(app: App): void {
  app.use(pinia);
  app.use(i18n);
  app.directive("ripple", {});
}

const OpenDialog = {
  props: { open: { type: Boolean, default: true } },
  setup(props: { open: boolean }) {
    const container = ref<HTMLElement | null>(null);
    useModal({ open: () => props.open, container, onClose: () => undefined });
    return () => (props.open ? h("div", { ref: container, role: "dialog" }, [h("p", "Delete the group?")]) : null);
  },
};

beforeEach(() => {
  pinia = createPinia();
  setActivePinia(pinia);
  i18n.global.locale.value = "en";
  keyHandlers = [];
  vi.stubGlobal("window", fakeWindow);
  vi.stubGlobal("document", testDocument);
  for (const name of ["Element", "HTMLElement", "HTMLButtonElement", "HTMLTextAreaElement"]) {
    vi.stubGlobal(name, class {});
  }
});

afterEach(() => {
  for (const entry of mounted) entry.unmount();
  mounted = [];
  vi.unstubAllGlobals();
});

describe("wizard shortcuts and open dialogs", () => {
  it("ignores the wizard shortcuts while a dialog without inputs is open, wherever focus is", () => {
    const wizard = useWizardStore();
    wizard.stepIndex = 1;
    mounted.push(mount(WizardView as never, install as never));
    mounted.push(mount(OpenDialog as never, install as never));

    press("ArrowLeft", { altKey: true });

    expect(wizard.stepIndex).toBe(1);
  });

  it("counts every open ModalDialog, not only dialogs that took focus", () => {
    const wizard = useWizardStore();
    wizard.stepIndex = 1;
    mounted.push(mount(WizardView as never, install as never));
    mounted.push(mount(ModalDialog as never, install as never, { open: true, title: "Delete group" }));

    press("ArrowLeft", { altKey: true });

    expect(wizard.stepIndex).toBe(1);
  });
});

describe("focus in a dialog", () => {
  it("moves into the dialog on open, stays inside on Tab and Shift+Tab and returns on close", async () => {
    const shown = reactive({ open: false });
    const Page = {
      setup() {
        const container = ref<HTMLElement | null>(null);
        useModal({ open: () => shown.open, container, onClose: () => (shown.open = false) });
        return () =>
          h("div", [
            h("button", { type: "button" }, "Delete frontend"),
            shown.open
              ? h("div", { ref: container, role: "dialog" }, [
                  h("p", "Delete the group frontend?"),
                  h("button", { type: "button" }, "Cancel"),
                  h("button", { type: "button" }, "Delete"),
                ])
              : null,
          ]);
      },
    };
    const page = mount(Page as never, install as never);
    mounted.push(page);
    const opener = buttonLabelled(page.root, "Delete frontend")!;
    opener.focus();

    shown.open = true;
    await nextTick();
    await nextTick();
    const cancel = buttonLabelled(page.root, "Cancel")!;
    const remove = findAll(page.root, "button").find((button) => textOf(button) === "Delete")!;
    const afterOpen = focused();
    press("Tab");
    const afterTab = focused();
    press("Tab");
    const afterWrap = focused();
    press("Tab", { shiftKey: true });
    const afterShiftTab = focused();
    press("Escape");
    await nextTick();
    await nextTick();

    expect(afterOpen).toBe(cancel);
    expect(afterTab).toBe(remove);
    expect(afterWrap).toBe(cancel);
    expect(afterShiftTab).toBe(remove);
    expect(focused()).toBe(opener);
  });
});
