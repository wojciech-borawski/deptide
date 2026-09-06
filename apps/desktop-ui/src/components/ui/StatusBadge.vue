<script setup lang="ts">
import { useI18n } from "vue-i18n";

import type { JobStatus } from "@/api/types";

const props = defineProps<{ status: JobStatus }>();
const { t } = useI18n();
</script>

<template>
  <span class="badge" :class="`badge-${props.status}`">
    <span v-if="props.status === 'running'" class="spinner small" />
    <span v-else class="dot" />
    {{ t(`status.${props.status}`) }}
  </span>
</template>

<style scoped>
.dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: currentColor;
}

.spinner.small {
  width: 10px;
  height: 10px;
  border-width: 1.5px;
  border-color: rgba(127, 127, 127, 0.25);
  border-top-color: currentColor;
}
</style>
