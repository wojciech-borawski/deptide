<script setup lang="ts">
import { useEventListener } from "@vueuse/core";

import AppIcon from "./AppIcon.vue";

const props = withDefaults(defineProps<{ open: boolean; title: string; width?: string }>(), {
  width: "720px",
});
const emit = defineEmits<{ close: [] }>();

useEventListener(window, "keydown", (event: KeyboardEvent) => {
  if (event.key === "Escape" && props.open) emit("close");
});
</script>

<template>
  <Teleport to="body">
    <transition name="fade">
      <div v-if="props.open" class="backdrop" @mousedown.self="emit('close')">
        <div class="dialog" :style="{ width: props.width }" role="dialog" :aria-label="props.title">
          <header class="dialog-header">
            <h2>{{ props.title }}</h2>
            <button class="btn btn-ghost btn-icon" type="button" aria-label="Close" @click="emit('close')">
              <AppIcon name="close" :size="16" />
            </button>
          </header>
          <div class="dialog-body"><slot /></div>
          <footer v-if="$slots.footer" class="dialog-footer"><slot name="footer" /></footer>
        </div>
      </div>
    </transition>
  </Teleport>
</template>

<style scoped>
.backdrop {
  position: fixed;
  inset: 0;
  background: rgba(5, 6, 10, 0.65);
  display: grid;
  place-items: center;
  z-index: 50;
  padding: 24px;
}

.dialog {
  max-width: 100%;
  max-height: calc(100vh - 48px);
  display: flex;
  flex-direction: column;
  background: var(--bg-panel);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow);
  overflow: hidden;
}

.dialog-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 14px 18px;
  border-bottom: 1px solid var(--border);
}

.dialog-body {
  padding: 16px 18px;
  overflow: auto;
  min-height: 0;
}

.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
  padding: 12px 18px;
  border-top: 1px solid var(--border);
  background: var(--bg-elevated);
}
</style>
