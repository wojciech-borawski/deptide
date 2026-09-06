<script setup lang="ts">
const props = defineProps<{ percent: number; done: boolean }>();
</script>

<template>
  <div class="progress" :class="{ done: props.done }">
    <div class="bar" :style="{ width: `${props.percent}%` }" />
  </div>
</template>

<style scoped>
.progress {
  height: 3px;
  background: var(--bg-elevated);
}

.bar {
  height: 100%;
  background: var(--gradient);
  transition: width 0.3s ease;
}

.progress.done .bar {
  background: var(--accent);
}
</style>
