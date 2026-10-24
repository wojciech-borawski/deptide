import { onBeforeUnmount, ref, watch, type Ref } from "vue";
import { useEventListener } from "@vueuse/core";

const FOCUSABLE = "a[href], button, input, select, textarea, [tabindex]";

const openModals = ref<symbol[]>([]);

export function isModalOpen(): boolean {
  return openModals.value.length > 0;
}

function focusablesIn(container: HTMLElement): HTMLElement[] {
  return [...container.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(
    (element) => !element.hasAttribute("disabled") && element.getAttribute("tabindex") !== "-1",
  );
}

function nextFocus(focusables: HTMLElement[], current: Element | null, backwards: boolean): HTMLElement | undefined {
  const index = focusables.findIndex((element) => element === current);
  if (backwards) return index <= 0 ? focusables.at(-1) : focusables[index - 1];
  return index === -1 || index === focusables.length - 1 ? focusables[0] : focusables[index + 1];
}

export function useModal(options: { open: () => boolean; container: Ref<HTMLElement | null>; onClose: () => void }) {
  const id = Symbol("modal");
  const isTopmost = () => openModals.value.at(-1) === id;
  let focusBeforeOpen: HTMLElement | null = null;

  function forget(): void {
    openModals.value = openModals.value.filter((entry) => entry !== id);
  }

  function focusInside(): void {
    const container = options.container.value;
    if (!container || container.contains(document.activeElement)) return;
    (focusablesIn(container)[0] ?? container).focus();
  }

  watch(
    options.open,
    (open, wasOpen) => {
      if (open) {
        openModals.value = [...openModals.value, id];
        focusBeforeOpen = document.activeElement as HTMLElement | null;
        focusInside();
      } else if (wasOpen) {
        forget();
        focusBeforeOpen?.focus();
        focusBeforeOpen = null;
      }
    },
    { immediate: true, flush: "post" },
  );

  onBeforeUnmount(forget);

  useEventListener(window, "keydown", (event: KeyboardEvent) => {
    if (!options.open() || !isTopmost()) return;
    if (event.key === "Escape") {
      options.onClose();
      return;
    }
    if (event.key !== "Tab") return;
    const container = options.container.value;
    if (!container) return;
    event.preventDefault();
    (nextFocus(focusablesIn(container), document.activeElement, event.shiftKey) ?? container).focus();
  });
}
