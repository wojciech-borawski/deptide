import { ref } from "vue";
import { defineStore } from "pinia";

import type { NoticeTone } from "@/components/ui/NoticeBanner.vue";

export interface Toast {
  id: number;
  tone: NoticeTone;
  text: string;
}

export interface ToastInput {
  tone?: NoticeTone;
  text: string;
  /** How long the toast stays, in milliseconds. Errors stay longer by default. */
  durationMs?: number;
}

const defaultDurationMs = 4500;
const errorDurationMs = 8000;
const maxVisible = 4;

let nextId = 1;

export const useToastStore = defineStore("toasts", () => {
  const toasts = ref<Toast[]>([]);
  const timers = new Map<number, ReturnType<typeof setTimeout>>();

  function dismiss(id: number): void {
    const timer = timers.get(id);
    if (timer) clearTimeout(timer);
    timers.delete(id);
    toasts.value = toasts.value.filter((toast) => toast.id !== id);
  }

  function push(input: ToastInput): number {
    const tone = input.tone ?? "info";
    const toast: Toast = { id: nextId++, tone, text: input.text };
    toasts.value = [...toasts.value, toast].slice(-maxVisible);

    const duration = input.durationMs ?? (tone === "error" ? errorDurationMs : defaultDurationMs);
    timers.set(
      toast.id,
      setTimeout(() => dismiss(toast.id), duration),
    );
    return toast.id;
  }

  function clear(): void {
    for (const toast of toasts.value) dismiss(toast.id);
  }

  return { toasts, push, dismiss, clear };
});
