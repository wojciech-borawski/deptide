<script setup lang="ts">
import { useI18n } from "vue-i18n";

import type { StatusCounts } from "@/composables/useRunTracker";

const props = defineProps<{ statusCounts: StatusCounts; finished: number; total: number; parallel: number }>();
const { t } = useI18n();
</script>

<template>
  <div class="status-line">
    <slot />
    <span class="muted">
      {{
        t(
          "run.progress",
          {
            done: props.finished,
            total: props.total,
            running: props.statusCounts.running,
            parallel: props.parallel,
          },
          props.total,
        )
      }}
    </span>
    <span class="spacer" />
    <span class="badge badge-ok">{{ props.statusCounts.ok }}</span>
    <span class="badge badge-warn">{{ props.statusCounts.warn }}</span>
    <span class="badge badge-failed">{{ props.statusCounts.failed }}</span>
    <span class="badge badge-skipped">{{ props.statusCounts.skipped }}</span>
  </div>
</template>

<style scoped>
.status-line {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
  font-size: 13px;
}
</style>
