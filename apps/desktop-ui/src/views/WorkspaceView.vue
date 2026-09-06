<script setup lang="ts">
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";

import AppIcon from "@/components/ui/AppIcon.vue";
import ActionButton from "@/components/ui/ActionButton.vue";
import NoticeBanner from "@/components/ui/NoticeBanner.vue";
import { formatDateTime } from "@/lib/format";
import { routeNames } from "@/router";
import { useUiStore } from "@/stores/ui";
import { useWorkspaceStore } from "@/stores/workspace";

const workspace = useWorkspaceStore();
const ui = useUiStore();
const router = useRouter();
const { t } = useI18n();

const holds = [
  { file: "update-libs.json", key: "workspace.configFile" },
  { file: "settings.json", key: "workspace.settingsFile" },
  { file: "runs/", key: "workspace.runsFolder" },
  { file: "logs/", key: "workspace.logsFolder" },
  { file: "backups/", key: "workspace.backupsFolder" },
  { file: "transfers/", key: "workspace.transfersFolder" },
];

const features = ["update", "safety", "insight", "transfer", "terminal", "comfort"];

async function openPath(path: string): Promise<void> {
  if (await workspace.open(path)) {
    await router.push({ name: routeNames.wizard });
  }
}

async function chooseFolder(): Promise<void> {
  const chosen = await openDialog({ directory: true, multiple: false, title: t("workspace.dialogTitle") });
  if (typeof chosen === "string") await openPath(chosen);
}
</script>

<template>
  <div class="workspace">
    <div class="hero">
      <div class="logo"><AppIcon name="package" :size="30" /></div>
      <h1>
        {{ t("app.name") }} <span class="version mono">{{ ui.info.version }}</span>
      </h1>
      <p class="muted lead">{{ t("app.tagline") }}</p>
      <ActionButton class="big" variant="primary" icon="folder" :busy="workspace.loading" @click="chooseFolder">
        {{ t("workspace.open") }}
      </ActionButton>
      <NoticeBanner v-if="workspace.error" tone="error" selectable>{{ workspace.error }}</NoticeBanner>
    </div>

    <section class="card">
      <div class="card-title">
        <h3>{{ t("workspace.features") }}</h3>
      </div>
      <ul class="features">
        <li v-for="feature in features" :key="feature">{{ t(`workspace.featureList.${feature}`) }}</li>
      </ul>
    </section>

    <div class="columns">
      <section class="card">
        <div class="card-title">
          <h3>{{ t("workspace.recent") }}</h3>
        </div>
        <p v-if="!workspace.recent.length" class="muted">{{ t("workspace.nothingOpened") }}</p>
        <div v-else class="list">
          <button
            v-for="entry in workspace.recent"
            :key="entry.path"
            type="button"
            class="list-row recent"
            :class="{ selected: entry.path === workspace.root }"
            @click="openPath(entry.path)"
          >
            <AppIcon name="folder" :size="16" />
            <span class="details">
              <span class="mono truncate">{{ entry.path }}</span>
              <span class="muted small">{{ t("workspace.opened", { date: formatDateTime(entry.lastOpened) }) }}</span>
            </span>
            <AppIcon name="chevronRight" :size="16" />
          </button>
        </div>
      </section>

      <section class="card">
        <div class="card-title">
          <h3>{{ t("workspace.holds") }}</h3>
        </div>
        <dl class="facts">
          <template v-for="entry in holds" :key="entry.file">
            <dt class="mono">{{ entry.file }}</dt>
            <dd>{{ t(entry.key) }}</dd>
          </template>
        </dl>
        <p class="muted small">
          <i18n-t keypath="workspace.legacyHint" tag="span">
            <template #path><span class="mono">config/update-libs.json</span></template>
          </i18n-t>
        </p>
      </section>
    </div>
  </div>
</template>

<style scoped>
.workspace {
  height: 100%;
  overflow: auto;
  padding: 48px 32px;
  display: flex;
  flex-direction: column;
  gap: 32px;
  max-width: 980px;
  margin: 0 auto;
  width: 100%;
}

.hero {
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
  gap: 12px;
}

.logo {
  display: grid;
  place-items: center;
  width: 64px;
  height: 64px;
  border-radius: 18px;
  background: var(--gradient);
  color: #0b0d12;
  margin-bottom: 4px;
}

.hero h1 {
  font-size: 28px;
}

.version {
  font-size: 13px;
  font-weight: 500;
  color: var(--text-muted);
  vertical-align: middle;
  margin-left: 6px;
}

.lead {
  max-width: 520px;
  font-size: 15px;
}

.big {
  padding: 12px 22px;
  font-size: 15px;
  margin-top: 8px;
}

.columns {
  display: grid;
  grid-template-columns: 1.2fr 1fr;
  gap: 16px;
}

.recent {
  width: 100%;
  border: none;
  border-bottom: 1px solid var(--border);
  background: transparent;
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: pointer;
}

.recent:last-child {
  border-bottom: none;
}

.details {
  display: flex;
  flex-direction: column;
  min-width: 0;
  flex: 1;
}

.small {
  font-size: 12px;
}

.facts {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 10px 14px;
  margin: 0 0 12px;
  font-size: 13px;
}

.facts dt {
  color: var(--accent);
}

.facts dd {
  margin: 0;
  color: var(--text-muted);
}

.features {
  margin: 0;
  padding-left: 18px;
  display: grid;
  gap: 6px;
  color: var(--text-muted);
  font-size: 13px;
}

.features li::marker {
  color: var(--accent);
}
</style>
