import { computed, ref, type Ref } from "vue";

import type { ProjectKind, ProjectView } from "@/api/types";

export type KindFilter = "all" | ProjectKind;

export const kindFilters: readonly KindFilter[] = ["all", "library", "application"];

export function useProjectFilter(projects: Ref<ProjectView[]>) {
  const text = ref("");
  const kind = ref<KindFilter>("all");

  const visible = computed(() => {
    const needle = text.value.trim().toLowerCase();
    return projects.value.filter((project) => {
      if (kind.value !== "all" && project.kind !== kind.value) return false;
      return !needle || project.name.toLowerCase().includes(needle) || project.path.toLowerCase().includes(needle);
    });
  });

  return { text, kind, visible };
}
