import { ref } from "vue";

export function describeError(cause: unknown): string {
  return String(cause);
}

export function useAsyncAction() {
  const busy = ref(false);
  const error = ref("");

  async function run<T>(work: () => Promise<T>): Promise<T | undefined> {
    busy.value = true;
    error.value = "";
    try {
      return await work();
    } catch (cause) {
      error.value = describeError(cause);
      return undefined;
    } finally {
      busy.value = false;
    }
  }

  return { busy, error, run };
}
