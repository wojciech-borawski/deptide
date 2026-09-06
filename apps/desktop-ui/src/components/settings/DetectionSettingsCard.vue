<script setup lang="ts">
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useI18n } from "vue-i18n";

import AppIcon from "@/components/ui/AppIcon.vue";
import { defaultBranchPattern, useSettingsDraftStore } from "@/stores/settings-draft";
import { useWorkspaceStore } from "@/stores/workspace";

const draft = useSettingsDraftStore();
const workspace = useWorkspaceStore();
const { t } = useI18n();

async function chooseRoot(): Promise<void> {
  const chosen = await openDialog({
    directory: true,
    multiple: false,
    defaultPath: draft.form.projectsRoot || workspace.root,
    title: t("settings.rootDialog"),
  });
  if (typeof chosen === "string") draft.form.projectsRoot = chosen;
}
</script>

<template>
  <section class="card stack">
    <div class="card-title">
      <h3>{{ t("settings.detection") }}</h3>
    </div>
    <div class="field">
      <label>{{ t("settings.projectsRoot") }}</label>
      <div class="row">
        <input v-model="draft.form.projectsRoot" class="input mono" placeholder="C:\example\repos" />
        <button class="btn" type="button" @click="chooseRoot">
          <AppIcon name="folder" :size="15" />
          {{ t("settings.choose") }}
        </button>
      </div>
      <span class="hint">
        {{ t("settings.rootHint") }}
        <span v-if="draft.rootMissing" class="warn-text">{{ t("settings.rootRequired") }}</span>
      </span>
    </div>
    <div class="grid-2">
      <div class="field">
        <label>{{ t("settings.scanDepth") }}</label>
        <input v-model.number="draft.form.scanDepth" class="input number" type="number" min="1" max="20" />
        <span class="hint">{{ t("settings.scanDepthHint") }}</span>
      </div>
      <div class="field">
        <label>{{ t("settings.ignored") }}</label>
        <input v-model="draft.ignoredText" class="input mono" placeholder=".vite-deps, tmp" />
        <span class="hint">{{ t("settings.ignoredHint") }}</span>
      </div>
    </div>
    <div class="field">
      <label>{{ t("settings.branchPattern") }}</label>
      <input v-model="draft.form.branchSuffixPattern" class="input mono" :placeholder="defaultBranchPattern" />
      <span class="hint">{{ t("settings.branchPatternHint", { pattern: defaultBranchPattern }) }}</span>
    </div>
  </section>
</template>

<style scoped>
.number {
  width: 120px;
}

.warn-text {
  color: var(--warn);
}
</style>
