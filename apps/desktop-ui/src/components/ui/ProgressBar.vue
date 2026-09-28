<script setup lang="ts">
const props = withDefaults(defineProps<{ percent: number; done: boolean; indeterminate?: boolean }>(), {
  indeterminate: false,
});
</script>

<template>
  <div class="progress" :class="{ done: props.done, indeterminate: props.indeterminate }">
    <div class="bar" :style="props.indeterminate ? undefined : { width: `${props.percent}%` }" />
  </div>
</template>

<style scoped>
.progress {
  height: 3px;
  background: var(--bg-elevated);
  overflow: hidden;
}

.bar {
  height: 100%;
  background: var(--gradient);
  transition: width 0.3s ease;
}

.progress.done .bar {
  background: var(--accent);
}

.progress.indeterminate .bar {
  width: 30%;
  animation: slide 1.2s ease-in-out infinite;
}

@keyframes slide {
  from {
    transform: translateX(-100%);
  }
  to {
    transform: translateX(340%);
  }
}
</style>
