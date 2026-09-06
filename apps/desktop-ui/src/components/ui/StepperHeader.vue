<script setup lang="ts">
import AppIcon from "./AppIcon.vue";

interface StepDescriptor {
  title: string;
  hint: string;
}

const props = defineProps<{ steps: readonly StepDescriptor[]; current: number }>();
const emit = defineEmits<{ select: [index: number] }>();
</script>

<template>
  <ol class="stepper">
    <li
      v-for="(step, index) in props.steps"
      :key="step.title"
      class="step"
      :class="{ done: index < props.current, active: index === props.current }"
      @click="index < props.current && emit('select', index)"
    >
      <span class="bullet">
        <AppIcon v-if="index < props.current" name="check" :size="14" />
        <template v-else>{{ index + 1 }}</template>
      </span>
      <span class="text">
        <span class="title">{{ step.title }}</span>
        <span class="hint">{{ step.hint }}</span>
      </span>
      <span v-if="index < props.steps.length - 1" class="connector" />
    </li>
  </ol>
</template>

<style scoped>
.stepper {
  display: flex;
  list-style: none;
  margin: 0;
  padding: 0;
  gap: 8px;
}

.step {
  display: flex;
  align-items: center;
  gap: 10px;
  flex: 1;
  color: var(--text-faint);
  min-width: 0;
}

.step.done {
  cursor: pointer;
  color: var(--text-muted);
}

.step.active {
  color: var(--text);
}

.bullet {
  display: grid;
  place-items: center;
  width: 28px;
  height: 28px;
  border-radius: 50%;
  border: 1.5px solid var(--border-strong);
  font-size: 12.5px;
  font-weight: 700;
  flex-shrink: 0;
  transition: all 0.15s ease;
}

.step.active .bullet {
  background: var(--gradient);
  border-color: transparent;
  color: #0b0d12;
}

.step.done .bullet {
  background: var(--accent-soft);
  border-color: var(--accent);
  color: var(--accent);
}

.text {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.title {
  font-weight: 600;
  font-size: 13px;
}

.hint {
  font-size: 11.5px;
  color: var(--text-faint);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.connector {
  flex: 1;
  height: 1px;
  background: var(--border-strong);
  margin: 0 6px;
}

.step.done .connector {
  background: var(--accent);
  opacity: 0.6;
}
</style>
