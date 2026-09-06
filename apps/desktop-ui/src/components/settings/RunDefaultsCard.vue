<script setup lang="ts">
import { useI18n } from "vue-i18n";

import type { ExecutionMode } from "@/api/types";
import { allSteps } from "@/api/types";
import { useSettingsDraftStore } from "@/stores/settings-draft";

const draft = useSettingsDraftStore();
const { t } = useI18n();

const modes: ExecutionMode[] = ["per-project", "per-step"];
</script>

<template>
  <section class="card stack">
    <div class="card-title">
      <h3>{{ t("settings.defaults") }}</h3>
    </div>
    <div class="grid-3">
      <div class="field">
        <label>{{ t("settings.steps") }}</label>
        <div class="stack tight">
          <label v-for="step in allSteps" :key="step" class="check">
            <input type="checkbox" :checked="draft.hasStep(step)" @change="draft.toggleStep(step)" />
            <span>{{ t(`steps.${step}`) }}</span>
          </label>
        </div>
      </div>
      <div class="field">
        <label>{{ t("settings.order") }}</label>
        <div class="stack tight">
          <label v-for="mode in modes" :key="mode" class="check">
            <input v-model="draft.form.mode" type="radio" :value="mode" />
            <span>{{ mode === "per-step" ? t("options.perStep") : t("options.perProject") }}</span>
          </label>
        </div>
      </div>
      <div class="field">
        <label>{{ t("settings.parallel") }}</label>
        <input v-model.number="draft.form.concurrency" class="input number" type="number" min="1" max="32" />
      </div>
    </div>
  </section>
</template>

<style scoped>
.number {
  width: 120px;
}

.tight {
  gap: 8px;
}
</style>
