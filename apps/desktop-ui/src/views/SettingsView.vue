<script setup lang="ts">
import { ref, watch } from "vue";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useI18n } from "vue-i18n";

import { errorLogPath } from "@/api/commands";
import PageHeader from "@/components/layout/PageHeader.vue";
import AboutCard from "@/components/settings/AboutCard.vue";
import AppearanceCard from "@/components/settings/AppearanceCard.vue";
import DetectionSettingsCard from "@/components/settings/DetectionSettingsCard.vue";
import NpmArgumentsCard from "@/components/settings/NpmArgumentsCard.vue";
import RunDefaultsCard from "@/components/settings/RunDefaultsCard.vue";
import UpdatesCard from "@/components/settings/UpdatesCard.vue";
import ActionButton from "@/components/ui/ActionButton.vue";
import AppIcon from "@/components/ui/AppIcon.vue";
import NoticeBanner from "@/components/ui/NoticeBanner.vue";
import { useAsyncAction } from "@/composables/useAsyncAction";
import { useSettingsDraftStore } from "@/stores/settings-draft";
import { useWorkspaceStore } from "@/stores/workspace";

const workspace = useWorkspaceStore();
const draft = useSettingsDraftStore();
const { t } = useI18n();

const saveAction = useAsyncAction();
const message = ref("");

function loadDraft(): void {
  if (workspace.settings && workspace.config) draft.load(workspace.settings, workspace.config);
}

async function save(): Promise<void> {
  const config = workspace.config;
  if (!config) return;

  // Read both forms before the first await: saving the settings replaces the
  // workspace snapshot, which re-loads the draft from the old config and would
  // otherwise wipe the edited npm arguments and transfer patterns.
  const settings = draft.toSettings();
  const nextConfig = draft.toConfig(config);

  message.value = "";
  await saveAction.run(async () => {
    await workspace.updateSettings(settings);
    await workspace.updateConfig(nextConfig);
    message.value = t("settings.saved");
    loadDraft();
  });
}

async function revealConfig(): Promise<void> {
  if (workspace.snapshot) await revealItemInDir(workspace.snapshot.configFile);
}

async function revealErrorLog(): Promise<void> {
  const path = await errorLogPath();
  if (!path) {
    message.value = t("settings.noErrorLog");
    return;
  }

  try {
    await revealItemInDir(path);
  } catch {
    message.value = t("settings.errorLogAt", { path });
  }
}

watch(() => workspace.snapshot, loadDraft, { immediate: true });
</script>

<template>
  <div class="page">
    <PageHeader :title="t('settings.title')" :subtitle="workspace.snapshot?.configFile">
      <button class="btn btn-ghost btn-sm" type="button" @click="revealConfig">
        <AppIcon name="external" :size="14" />
        {{ t("settings.showConfig") }}
      </button>
      <button class="btn btn-ghost btn-sm" type="button" :title="t('settings.errorLogTitle')" @click="revealErrorLog">
        <AppIcon name="warning" :size="14" />
        {{ t("settings.errorLog") }}
      </button>
      <ActionButton variant="primary" icon="check" :busy="saveAction.busy.value" @click="save">
        {{ t("common.save") }}
      </ActionButton>
    </PageHeader>

    <div class="body stack">
      <NoticeBanner v-if="message" tone="ok">{{ message }}</NoticeBanner>
      <NoticeBanner v-if="saveAction.error.value" tone="error" selectable>{{ saveAction.error.value }}</NoticeBanner>

      <DetectionSettingsCard />
      <RunDefaultsCard />
      <NpmArgumentsCard />
      <AppearanceCard />
      <AboutCard />
      <UpdatesCard />
    </div>
  </div>
</template>

<style scoped>
.page {
  display: flex;
  flex-direction: column;
  height: 100%;
}

.body {
  flex: 1;
  overflow: auto;
  padding: 20px 24px;
  max-width: 980px;
}
</style>
