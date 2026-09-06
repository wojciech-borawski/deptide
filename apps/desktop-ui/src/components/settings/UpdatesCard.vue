<script setup lang="ts">
import { openUrl } from "@tauri-apps/plugin-opener";
import { useI18n } from "vue-i18n";

import ActionButton from "@/components/ui/ActionButton.vue";
import NoticeBanner from "@/components/ui/NoticeBanner.vue";
import { useUiStore } from "@/stores/ui";

const ui = useUiStore();
const { t } = useI18n();

async function openDownload(): Promise<void> {
  if (ui.update?.url) await openUrl(ui.update.url);
}
</script>

<template>
  <section class="card stack">
    <div class="card-title">
      <h3>{{ t("settings.updates") }}</h3>
      <span class="muted">{{ t("settings.currentVersion", { version: ui.info.version }) }}</span>
    </div>
    <div class="field">
      <label>{{ t("settings.updateUrl") }}</label>
      <div class="row">
        <input v-model="ui.updateUrl" class="input mono" placeholder="https://example.com/deptide/latest.json" />
        <ActionButton :busy="ui.checkingUpdate" :disabled="!ui.updateUrl.trim()" @click="ui.checkForUpdate">
          {{ t("settings.checkNow") }}
        </ActionButton>
      </div>
      <span class="hint">{{ t("settings.updateUrlHint") }}</span>
    </div>
    <NoticeBanner v-if="ui.updateError" tone="error" selectable>{{ ui.updateError }}</NoticeBanner>
    <div v-else-if="ui.update" class="notice row" :class="ui.update.newer ? 'notice-warn' : 'notice-ok'">
      <span>
        {{
          ui.update.newer
            ? t("settings.newer", { latest: ui.update.latest, current: ui.update.current })
            : t("settings.upToDate", { version: ui.update.current })
        }}
        <span v-if="ui.update.notes" class="muted"> · {{ ui.update.notes }}</span>
      </span>
      <span class="spacer" />
      <button v-if="ui.update.newer && ui.update.url" class="btn btn-sm" type="button" @click="openDownload">
        {{ t("settings.openDownload") }}
      </button>
    </div>
  </section>
</template>
