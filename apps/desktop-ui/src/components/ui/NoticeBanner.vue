<script setup lang="ts">
export type NoticeTone = "info" | "ok" | "warn" | "error";

const props = withDefaults(defineProps<{ tone?: NoticeTone; selectable?: boolean }>(), {
  tone: "info",
  selectable: false,
});
</script>

<template>
  <p class="notice" :class="[`notice-${props.tone}`, { selectable: props.selectable }]">
    <slot />
  </p>
</template>
