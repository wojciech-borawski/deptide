<script setup lang="ts">
import { computed, ref, useSlots } from "vue";
import { useResizeObserver } from "@vueuse/core";

/**
 * Two panes side by side with a draggable, keyboard-operable divider. `modelValue` is the start pane's share of the
 * width in percent. The divider shows only when the `end` slot is filled and the width is at least `stackBelow`;
 * narrower, the panes stack and the divider is hidden. `stackBelow` 0 never stacks.
 */
const props = withDefaults(
  defineProps<{
    modelValue: number;
    label: string;
    defaultValue?: number;
    minStart?: number;
    minEnd?: number;
    stackBelow?: number;
  }>(),
  { defaultValue: 32, minStart: 220, minEnd: 360, stackBelow: 0 },
);
const emit = defineEmits<{ "update:modelValue": [value: number] }>();
const slots = useSlots();

const handleSize = 14;
const step = 2;
const bigStep = 10;
const unmeasured = { min: 5, max: 95 };

const container = ref<HTMLElement | null>(null);
const width = ref(0);
const dragging = ref(false);

const stacked = computed(() => props.stackBelow > 0 && width.value > 0 && width.value < props.stackBelow);
const bounds = computed(() => {
  if (width.value <= 0) return unmeasured;
  const min = round((props.minStart * 100) / width.value);
  const max = round(100 - ((props.minEnd + handleSize) * 100) / width.value);
  return { min, max: Math.max(min, max) };
});
const current = computed(() => clamp(props.modelValue));

useResizeObserver(container, ([entry]) => {
  if (entry) width.value = entry.contentRect.width;
});

function round(value: number): number {
  return Math.round(value * 10) / 10;
}

function clamp(value: number): number {
  return round(Math.min(bounds.value.max, Math.max(bounds.value.min, value)));
}

function showHandle(): boolean {
  return !!slots.end && !stacked.value;
}

function measure(): DOMRect | null {
  const rect = container.value?.getBoundingClientRect() ?? null;
  if (rect) width.value = rect.width;
  return rect;
}

function move(value: number): void {
  const next = clamp(value);
  if (next !== props.modelValue) emit("update:modelValue", next);
}

function onKeydown(event: KeyboardEvent): void {
  if (event.altKey) return;
  measure();
  const distance = event.shiftKey ? bigStep : step;
  const targets = new Map([
    ["ArrowLeft", current.value - distance],
    ["ArrowRight", current.value + distance],
    ["Home", bounds.value.min],
    ["End", bounds.value.max],
  ]);
  const next = targets.get(event.key);
  if (next === undefined) return;
  event.preventDefault();
  move(next);
}

function onPointerdown(event: PointerEvent): void {
  if (event.button !== 0) return;
  event.preventDefault();
  measure();
  const grip = event.currentTarget as HTMLElement;
  grip.setPointerCapture(event.pointerId);
  grip.focus();
  dragging.value = true;
}

function onPointermove(event: PointerEvent): void {
  if (!dragging.value) return;
  const rect = measure();
  if (rect && rect.width > 0) move(((event.clientX - rect.left - handleSize / 2) * 100) / rect.width);
}

function reset(): void {
  measure();
  move(props.defaultValue);
}

function stopDragging(): void {
  dragging.value = false;
}
</script>

<template>
  <div ref="container" class="split" :class="{ stacked, dragging }">
    <div class="pane start" :style="showHandle() ? { flex: `0 1 ${current}%`, minWidth: `${props.minStart}px` } : {}">
      <slot name="start" />
    </div>
    <div
      v-if="showHandle()"
      class="handle"
      role="separator"
      aria-orientation="vertical"
      :tabindex="0"
      :aria-label="props.label"
      :aria-valuenow="Math.round(current)"
      :aria-valuemin="Math.ceil(bounds.min)"
      :aria-valuemax="Math.floor(bounds.max)"
      :style="{ width: `${handleSize}px` }"
      @keydown="onKeydown"
      @pointerdown="onPointerdown"
      @pointermove="onPointermove"
      @pointerup="stopDragging"
      @pointercancel="stopDragging"
      @lostpointercapture="stopDragging"
      @dblclick="reset"
    />
    <div v-if="slots.end" class="pane end" :style="showHandle() ? { minWidth: `${props.minEnd}px` } : {}">
      <slot name="end" />
    </div>
  </div>
</template>

<style scoped>
.split {
  display: flex;
  flex: 1 1 0;
  min-width: 0;
  min-height: 0;
}

.split.dragging {
  cursor: col-resize;
  user-select: none;
}

.split.stacked {
  flex-direction: column;
  gap: 14px;
}

.pane {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
}

.start,
.end {
  flex: 1 1 0;
}

.stacked .start {
  flex: 0 1 35%;
}

.handle {
  position: relative;
  flex: none;
  cursor: col-resize;
  touch-action: none;
  outline: none;
}

.handle::before {
  content: "";
  position: absolute;
  top: 0;
  bottom: 0;
  left: 50%;
  width: 2px;
  transform: translateX(-50%);
  border-radius: 1px;
  background: var(--border-strong);
  transition: background 0.12s;
}

.handle:hover::before,
.dragging .handle::before,
.handle:focus-visible::before {
  background: var(--accent);
}

.handle:focus-visible::before {
  width: 4px;
}
</style>
