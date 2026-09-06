<script setup lang="ts">
import { useI18n } from "vue-i18n";

import { kindFilters, type KindFilter } from "@/composables/useProjectFilter";

const kind = defineModel<KindFilter>({ required: true });
const { t } = useI18n();
</script>

<template>
  <span class="kinds">
    <button
      v-for="option in kindFilters"
      :key="option"
      class="chip"
      :class="{ active: kind === option }"
      type="button"
      @click="kind = option"
    >
      {{ t(`projects.kindFilter.${option}`) }}
    </button>
  </span>
</template>

<style scoped>
.kinds {
  display: inline-flex;
  gap: 6px;
}
</style>
