<script setup lang="ts">
import { useI18n } from "vue-i18n";

import { useSettingsDraftStore } from "@/stores/settings-draft";

const draft = useSettingsDraftStore();
const { t } = useI18n();
</script>

<template>
  <section class="card stack">
    <div class="card-title">
      <h3>{{ t("settings.npmArgs") }}</h3>
      <span class="muted">{{ t("settings.storedIn") }}</span>
    </div>
    <div class="grid-2">
      <div class="field">
        <label>{{ t("settings.installArgs") }}</label>
        <input v-model="draft.installArgsText" class="input mono" placeholder="--no-fund" />
        <span class="hint">{{ t("settings.installArgsHint") }}</span>
      </div>
      <div class="field">
        <label>{{ t("settings.auditArgs") }}</label>
        <input v-model="draft.auditFixArgsText" class="input mono" placeholder="--force" />
      </div>
    </div>
    <div class="field">
      <label>{{ t("settings.transferIgnore") }}</label>
      <textarea
        v-model="draft.transferIgnoreText"
        class="textarea mono"
        rows="3"
        placeholder="*.local&#10;coverage/&#10;.env"
      ></textarea>
      <span class="hint">{{ t("settings.transferIgnoreHint") }}</span>
    </div>
  </section>
</template>
