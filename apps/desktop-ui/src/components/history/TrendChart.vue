<script setup lang="ts">
import { computed } from "vue";

import { formatDuration } from "@/lib/format";

export interface TrendPoint {
  label: string;
  value: number;
  tone?: "ok" | "warn" | "failed";
}

const props = withDefaults(defineProps<{ points: TrendPoint[]; height?: number; unit?: "duration" | "count" }>(), {
  height: 120,
  unit: "duration",
});

const width = 600;
const padding = { top: 12, right: 12, bottom: 22, left: 52 };

const maxValue = computed(() => Math.max(1, ...props.points.map((point) => point.value)));

const bars = computed(() => {
  const count = props.points.length;
  const innerWidth = width - padding.left - padding.right;
  const innerHeight = props.height - padding.top - padding.bottom;
  const slot = innerWidth / Math.max(1, count);
  const barWidth = Math.min(28, slot * 0.7);

  return props.points.map((point, index) => {
    const barHeight = (point.value / maxValue.value) * innerHeight;
    return {
      ...point,
      x: padding.left + slot * index + (slot - barWidth) / 2,
      y: padding.top + innerHeight - barHeight,
      width: barWidth,
      height: barHeight,
      labelX: padding.left + slot * index + slot / 2,
    };
  });
});

const ticks = computed(() => {
  const innerHeight = props.height - padding.top - padding.bottom;
  return [0, 0.5, 1].map((fraction) => ({
    y: padding.top + innerHeight - innerHeight * fraction,
    label: format(maxValue.value * fraction),
  }));
});

function format(value: number): string {
  return props.unit === "duration" ? formatDuration(value) : String(Math.round(value));
}

function fill(tone: TrendPoint["tone"]): string {
  if (tone === "failed") return "var(--failed)";
  if (tone === "warn") return "var(--warn)";
  return "var(--accent)";
}
</script>

<template>
  <svg class="trend" :viewBox="`0 0 ${width} ${props.height}`" preserveAspectRatio="none" role="img">
    <g v-for="tick in ticks" :key="tick.y">
      <line :x1="padding.left" :x2="width - padding.right" :y1="tick.y" :y2="tick.y" class="grid" />
      <text :x="padding.left - 6" :y="tick.y + 4" text-anchor="end" class="tick">{{ tick.label }}</text>
    </g>
    <g v-for="bar in bars" :key="bar.label + bar.x">
      <rect :x="bar.x" :y="bar.y" :width="bar.width" :height="bar.height" rx="3" :fill="fill(bar.tone)" opacity="0.9">
        <title>{{ bar.label }}: {{ format(bar.value) }}</title>
      </rect>
      <text :x="bar.labelX" :y="props.height - 6" text-anchor="middle" class="tick">{{ bar.label }}</text>
    </g>
  </svg>
</template>

<style scoped>
.trend {
  width: 100%;
  height: auto;
  display: block;
}

.grid {
  stroke: var(--border);
  stroke-width: 1;
}

.tick {
  fill: var(--text-faint);
  font-size: 10px;
  font-family: var(--font);
}
</style>
