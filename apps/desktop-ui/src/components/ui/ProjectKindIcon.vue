<script setup lang="ts">
import { useI18n } from "vue-i18n";

import type { ProjectKind } from "@/api/types";
import AppIcon, { type IconName } from "./AppIcon.vue";

const props = withDefaults(defineProps<{ kind: ProjectKind; size?: number }>(), { size: 16 });
const { t } = useI18n();

const icons: Record<ProjectKind, IconName> = {
  library: "package",
  application: "layers",
  unknown: "folder",
};
</script>

<template>
  <span class="kind" :class="props.kind" :title="t(`projects.kinds.${props.kind}`)">
    <AppIcon :name="icons[props.kind]" :size="props.size" />
  </span>
</template>

<style scoped>
.kind {
  display: inline-grid;
  place-items: center;
  width: 26px;
  height: 26px;
  border-radius: 8px;
  flex-shrink: 0;
}

.kind.library {
  background: var(--violet-soft);
  color: var(--violet);
}

.kind.application {
  background: var(--accent-soft);
  color: var(--accent);
}

.kind.unknown {
  background: var(--pending-soft);
  color: var(--pending);
}
</style>
