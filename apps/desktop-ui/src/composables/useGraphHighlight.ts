import { computed, ref } from "vue";

export function useGraphHighlight() {
  const hovered = ref<string | null>(null);
  const focused = ref<string | null>(null);
  const pinned = ref<string | null>(null);

  const active = computed(() => hovered.value ?? focused.value ?? pinned.value);

  function enter(name: string): void {
    hovered.value = name;
  }

  function leave(name: string): void {
    if (hovered.value === name) hovered.value = null;
  }

  function focus(name: string): void {
    focused.value = name;
  }

  function blur(name: string): void {
    if (focused.value === name) focused.value = null;
  }

  function toggle(name: string): void {
    pinned.value = pinned.value === name ? null : name;
  }

  function clear(): void {
    pinned.value = null;
  }

  return { active, pinned, enter, leave, focus, blur, toggle, clear };
}
