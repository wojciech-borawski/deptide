<script setup lang="ts">
import { useI18n } from "vue-i18n";

import AppIcon, { type IconName } from "./AppIcon.vue";
import type { NoticeTone } from "./NoticeBanner.vue";
import { useToastStore } from "@/stores/toasts";

const toasts = useToastStore();
const { t } = useI18n();

const icons: Record<NoticeTone, IconName> = {
  info: "bolt",
  ok: "check",
  warn: "warning",
  error: "warning",
};
</script>

<template>
  <div class="toast-host" aria-live="polite">
    <transition-group name="toast">
      <div v-for="toast in toasts.toasts" :key="toast.id" class="toast" :class="`toast-${toast.tone}`" role="status">
        <AppIcon :name="icons[toast.tone]" :size="16" />
        <span class="text selectable">{{ toast.text }}</span>
        <button class="dismiss" type="button" :aria-label="t('common.close')" @click="toasts.dismiss(toast.id)">
          <AppIcon name="close" :size="13" />
        </button>
      </div>
    </transition-group>
  </div>
</template>

<style scoped>
.toast-host {
  position: fixed;
  right: 20px;
  bottom: 20px;
  z-index: 60;
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 8px;
  max-width: min(440px, calc(100vw - 40px));
  pointer-events: none;
}

.toast {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 10px 12px 10px 14px;
  border-radius: var(--radius);
  border: 1px solid var(--border-strong);
  background: var(--bg-elevated);
  color: var(--text);
  box-shadow: var(--shadow);
  font-size: 13px;
  line-height: 1.4;
  pointer-events: auto;
  max-width: 100%;
}

.toast > :first-child {
  flex-shrink: 0;
  margin-top: 1px;
}

.toast-ok {
  border-color: rgba(74, 222, 128, 0.4);
}

.toast-ok > :first-child {
  color: var(--ok);
}

.toast-warn {
  border-color: rgba(251, 191, 36, 0.4);
}

.toast-warn > :first-child {
  color: var(--warn);
}

.toast-error {
  border-color: rgba(248, 113, 113, 0.45);
}

.toast-error > :first-child {
  color: var(--failed);
}

.toast-info > :first-child {
  color: var(--running);
}

.text {
  flex: 1;
  min-width: 0;
  overflow-wrap: anywhere;
}

.dismiss {
  display: grid;
  place-items: center;
  border: none;
  background: transparent;
  color: var(--text-muted);
  padding: 2px;
  cursor: pointer;
  border-radius: 4px;
  flex-shrink: 0;
}

.dismiss:hover {
  color: var(--text);
  background: var(--bg-hover);
}

.toast-enter-active,
.toast-leave-active,
.toast-move {
  transition:
    opacity 0.18s ease,
    transform 0.18s ease;
}

.toast-enter-from {
  opacity: 0;
  transform: translateY(10px);
}

.toast-leave-to {
  opacity: 0;
  transform: translateX(16px);
}

.toast-leave-active {
  position: absolute;
}
</style>
