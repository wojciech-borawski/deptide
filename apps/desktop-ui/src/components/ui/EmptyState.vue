<script setup lang="ts">
import AppIcon, { type IconName } from "./AppIcon.vue";

const props = defineProps<{ icon: IconName; title: string; text?: string }>();
</script>

<template>
  <div class="empty">
    <div class="icon"><AppIcon :name="props.icon" :size="28" /></div>
    <h2>{{ props.title }}</h2>
    <p v-if="props.text" class="muted">{{ props.text }}</p>
    <div v-if="$slots.default" class="actions"><slot /></div>
  </div>
</template>

<style scoped>
.empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
  gap: 10px;
  padding: 48px 24px;
  max-width: 460px;
  margin: 0 auto;
}

.icon {
  display: grid;
  place-items: center;
  width: 56px;
  height: 56px;
  border-radius: 16px;
  background: var(--accent-soft);
  color: var(--accent);
  margin-bottom: 6px;
}

.actions {
  display: flex;
  gap: 10px;
  margin-top: 8px;
}
</style>
