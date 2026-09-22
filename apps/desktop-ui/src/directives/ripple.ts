import type { Directive } from "vue";

// A small press feedback in the spirit of Vuetify's v-ripple: a circle grows
// from the pointer position and fades out. Works on any element; the element
// gets position: relative and overflow: hidden through the host class.

const waveClass = "ripple-wave";
const hostClass = "ripple-host";
const durationMs = 450;

function spawn(host: HTMLElement, x: number, y: number): void {
  const rect = host.getBoundingClientRect();
  const radius = Math.hypot(Math.max(x - rect.left, rect.right - x), Math.max(y - rect.top, rect.bottom - y));

  const wave = document.createElement("span");
  wave.className = waveClass;
  wave.style.width = `${radius * 2}px`;
  wave.style.height = `${radius * 2}px`;
  wave.style.left = `${x - rect.left - radius}px`;
  wave.style.top = `${y - rect.top - radius}px`;
  host.appendChild(wave);

  window.setTimeout(() => wave.remove(), durationMs);
}

function onPointerDown(this: HTMLElement, event: PointerEvent): void {
  if (event.button !== 0 || (this as HTMLButtonElement).disabled) return;
  spawn(this, event.clientX, event.clientY);
}

function onKeydown(this: HTMLElement, event: KeyboardEvent): void {
  if (event.repeat || (event.key !== "Enter" && event.key !== " ")) return;
  const rect = this.getBoundingClientRect();
  spawn(this, rect.left + rect.width / 2, rect.top + rect.height / 2);
}

export const ripple: Directive<HTMLElement> = {
  mounted(element) {
    element.classList.add(hostClass);
    element.addEventListener("pointerdown", onPointerDown);
    element.addEventListener("keydown", onKeydown);
  },
  unmounted(element) {
    element.removeEventListener("pointerdown", onPointerDown);
    element.removeEventListener("keydown", onKeydown);
    element.classList.remove(hostClass);
  },
};
