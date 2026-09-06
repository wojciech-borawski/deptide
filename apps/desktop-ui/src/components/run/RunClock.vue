<script setup lang="ts">
import { formatClock } from "@/lib/format";

const props = defineProps<{ elapsedMs: number; live: boolean }>();
</script>

<template>
  <span class="clock mono" :class="{ live: props.live }">{{ formatClock(props.elapsedMs) }}</span>
</template>

<style scoped>
.clock {
  font-size: 22px;
  font-weight: 700;
  color: var(--text-muted);
  padding: 0 6px;
}

.clock.live {
  color: var(--accent);
}
</style>
