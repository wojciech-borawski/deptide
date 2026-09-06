<script setup lang="ts">
import { useI18n } from "vue-i18n";

import AppIcon from "@/components/ui/AppIcon.vue";
import { useUiStore } from "@/stores/ui";
import { useWorkspaceStore } from "@/stores/workspace";

const ui = useUiStore();
const workspace = useWorkspaceStore();
const { t } = useI18n();
</script>

<template>
  <section class="card stack">
    <div class="card-title">
      <h3>{{ t("settings.about") }}</h3>
      <span class="badge badge-accent mono">{{ t("settings.aboutVersion", { version: ui.info.version }) }}</span>
    </div>
    <div class="about-body">
      <div class="logo"><AppIcon name="package" :size="26" /></div>
      <div class="stack tight">
        <strong class="app-name">{{ ui.info.name }}</strong>
        <p class="muted">{{ t("settings.aboutText") }}</p>
        <p class="muted small">{{ t("settings.aboutDocs") }}</p>
        <dl class="facts">
          <dt>{{ t("settings.aboutWorkspace") }}</dt>
          <dd class="mono selectable">{{ workspace.root }}</dd>
          <dt>{{ t("settings.aboutIdentifier") }}</dt>
          <dd class="mono">{{ ui.info.identifier }}</dd>
        </dl>
      </div>
    </div>
  </section>
</template>

<style scoped>
.tight {
  gap: 8px;
}

.about-body {
  display: flex;
  gap: 16px;
  align-items: flex-start;
}

.logo {
  display: grid;
  place-items: center;
  width: 48px;
  height: 48px;
  border-radius: 14px;
  background: var(--gradient);
  color: #0b0d12;
  flex-shrink: 0;
}

.app-name {
  font-size: 16px;
}

.small {
  font-size: 12px;
}

.facts {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 4px 14px;
  margin: 4px 0 0;
  font-size: 12.5px;
}

.facts dt {
  color: var(--text-muted);
}

.facts dd {
  margin: 0;
}
</style>
