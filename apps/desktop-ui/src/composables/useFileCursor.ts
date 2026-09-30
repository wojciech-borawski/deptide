import { computed, ref, useId, watch } from "vue";

import { fileListKey } from "@/lib/receive-view";
import { useTransferStore } from "@/stores/transfer";

export function useFileCursor(
  source: () => string,
  order: () => readonly string[],
  folders: () => ReadonlyMap<string, boolean> = () => new Map(),
) {
  const transfer = useTransferStore();
  const prefix = useId();
  const list = ref<HTMLElement | null>(null);
  const cursor = ref<string | null>(null);
  const ids = computed(() => new Map(order().map((path, index) => [path, `${prefix}-row-${index}`])));
  const activeId = computed(() => (cursor.value === null ? undefined : ids.value.get(cursor.value)));

  watch(
    () => transfer.previewFor(source()),
    (previewed) => {
      if (previewed !== null) cursor.value = previewed;
    },
    { immediate: true },
  );

  watch(
    activeId,
    (id) => {
      if (id) list.value?.querySelector(`#${id}`)?.scrollIntoView({ block: "nearest" });
    },
    { flush: "post" },
  );

  function idFor(path: string): string | undefined {
    return ids.value.get(path);
  }

  function onKeydown(event: KeyboardEvent): void {
    if (event.target !== event.currentTarget || event.altKey) return;
    const next = fileListKey(
      event.key,
      order(),
      { cursor: cursor.value, preview: transfer.previewFor(source()) },
      folders(),
    );
    if (!next) return;
    event.preventDefault();
    cursor.value = next.cursor;
    if (next.toggle !== undefined) transfer.toggleFolder(source(), next.toggle);
    transfer.setPreview(source(), next.preview);
  }

  return { prefix, list, cursor, activeId, idFor, onKeydown };
}
