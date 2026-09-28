<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";

import AppIcon from "@/components/ui/AppIcon.vue";
import { routeNames } from "@/router";

const props = defineProps<{ patterns: string[]; disabled: string[]; busy?: boolean }>();
const emit = defineEmits<{ toggle: [pattern: string] }>();
const router = useRouter();
const { t } = useI18n();

function isOn(pattern: string): boolean {
  return !props.disabled.includes(pattern);
}

function goToSettings(): void {
  void router.push({ name: routeNames.settings });
}
</script>

<template>
  <div class="field">
    <label>{{ t("transfer.ignoreGlobal") }}</label>
    <div class="chips">
      <button
        v-for="pattern in props.patterns"
        :key="pattern"
        class="chip mono"
        :class="{ active: isOn(pattern), off: !isOn(pattern) }"
        type="button"
        :aria-pressed="isOn(pattern)"
        :disabled="props.busy"
        :title="isOn(pattern) ? t('transfer.ignoreOn') : t('transfer.ignoreOff')"
        @click="emit('toggle', pattern)"
      >
        {{ pattern }}
      </button>
      <span v-if="!props.patterns.length" class="faint">-</span>
      <button class="chip" type="button" @click="goToSettings">
        <AppIcon name="settings" :size="12" />
        {{ t("nav.settings") }}
      </button>
    </div>
    <span class="hint">{{ t("transfer.ignoreGlobalHint") }}</span>
  </div>
</template>

<style scoped>
.chips {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.chip.off {
  color: var(--text-faint);
  text-decoration: line-through;
}

.chip:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}
</style>
