import { computed, type Ref } from "vue";

import type { ProjectView } from "@/api/types";

export interface SelectionSource {
  names: () => readonly string[];
  replace: (names: string[]) => void;
}

export function useProjectSelection(source: SelectionSource, visible: Ref<ProjectView[]>) {
  const selectable = computed(() => visible.value.filter((project) => project.exists));
  const allVisibleSelected = computed(
    () => selectable.value.length > 0 && selectable.value.every((project) => source.names().includes(project.name)),
  );

  function isSelected(name: string): boolean {
    return source.names().includes(name);
  }

  function toggle(name: string): void {
    const next = new Set(source.names());
    if (next.has(name)) next.delete(name);
    else next.add(name);
    source.replace([...next]);
  }

  function toggleAllVisible(): void {
    const next = new Set(source.names());
    for (const project of selectable.value) {
      if (allVisibleSelected.value) next.delete(project.name);
      else next.add(project.name);
    }
    source.replace([...next]);
  }

  return { selectable, allVisibleSelected, isSelected, toggle, toggleAllVisible };
}
