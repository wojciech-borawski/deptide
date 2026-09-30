<script setup lang="ts">
import { useI18n } from "vue-i18n";

export type PillState = "take" | "keep" | "mixed";

const props = defineProps<{ state: PillState; index: number; total: number }>();
const emit = defineEmits<{ toggle: [] }>();
const { t } = useI18n();

const labels: Record<PillState, string> = {
  take: "transfer.filePreview.take",
  keep: "transfer.filePreview.keep",
  mixed: "transfer.filePreview.mixed",
};
</script>

<template>
  <button
    type="button"
    class="chunk-pill"
    :class="props.state"
    :aria-label="
      t('transfer.filePreview.chunkLabel', {
        index: props.index + 1,
        total: props.total,
        state: t(labels[props.state]),
      })
    "
    :title="t('transfer.filePreview.chunkTitle', { index: props.index + 1, total: props.total })"
    @click="emit('toggle')"
  >
    {{ t(labels[props.state]) }}
  </button>
</template>

<style scoped>
.chunk-pill {
  padding: 0 10px;
  border: 1px dashed var(--border-strong);
  border-radius: 999px;
  background: var(--bg-panel);
  color: var(--text-muted);
  font-family: var(--font);
  font-size: 11px;
  line-height: 18px;
  cursor: pointer;
}

.chunk-pill:hover {
  border-color: var(--accent);
}

.chunk-pill:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}

.chunk-pill.take {
  border-style: solid;
  border-color: var(--accent);
  background: var(--accent-soft);
  color: var(--accent);
  font-weight: 600;
}

.chunk-pill.mixed {
  border-color: var(--accent);
  color: var(--accent);
}
</style>
