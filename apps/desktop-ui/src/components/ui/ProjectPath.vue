<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";

import { joinPath, type PathSeparator } from "@/lib/project-paths";

const props = defineProps<{
  prefix: string[];
  rest: string[];
  separator: PathSeparator;
  grouped: boolean;
  fullPath: string;
}>();
const { t } = useI18n();

const expanded = ref(false);
const prefixText = computed(() => joinPath(props.prefix, props.separator));
const lastIndex = computed(() => props.rest.length - 1);

function toggle(): void {
  expanded.value = !expanded.value;
}
</script>

<template>
  <span class="path mono selectable" :title="props.fullPath">
    <template v-if="props.prefix.length">
      <button
        v-if="!expanded"
        class="ellipsis"
        type="button"
        :title="t('projects.showFullPath', { path: prefixText })"
        @click.prevent.stop="toggle"
      >
        […]
      </button>
      <button v-else class="prefix" type="button" :title="t('projects.hideFullPath')" @click.prevent.stop="toggle">
        {{ prefixText }}
      </button>
      <span class="sep">{{ props.separator }}</span>
    </template>
    <template v-for="(segment, index) in props.rest" :key="index">
      <span v-if="index > 0" class="sep">{{ props.separator }}</span>
      <span :class="{ folder: props.grouped && index < lastIndex }">{{ segment }}</span>
    </template>
  </span>
</template>

<style scoped>
.path {
  display: inline-flex;
  align-items: baseline;
  flex-wrap: wrap;
  min-width: 0;
  font-size: 12px;
  color: var(--text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ellipsis,
.prefix {
  border: none;
  background: transparent;
  padding: 0 2px;
  margin: 0 -2px;
  border-radius: 3px;
  color: var(--text-muted);
  font: inherit;
  cursor: pointer;
}

.ellipsis {
  color: var(--accent);
  font-weight: 600;
}

.ellipsis:hover,
.prefix:hover {
  background: var(--accent-soft);
  color: var(--accent);
}

.sep {
  opacity: 0.6;
}

.folder {
  color: var(--accent);
  font-weight: 600;
}
</style>
