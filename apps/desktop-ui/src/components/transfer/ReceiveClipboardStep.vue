<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import type { ClipboardEntry } from "@/api/types";
import AppIcon from "@/components/ui/AppIcon.vue";
import ActionButton from "@/components/ui/ActionButton.vue";
import ProgressBar from "@/components/ui/ProgressBar.vue";
import { downloadPercent } from "@/lib/clipboard-download";
import { formatBytes } from "@/lib/format";
import { useTransferStore } from "@/stores/transfer";
import { useUiStore, type ClipboardLayout } from "@/stores/ui";
import { useWorkspaceStore } from "@/stores/workspace";
import IgnorePatternChips from "./IgnorePatternChips.vue";

const workspace = useWorkspaceStore();
const transfer = useTransferStore();
const ui = useUiStore();
const { t } = useI18n();

const layouts: { layout: ClipboardLayout; label: string }[] = [
  { layout: "list", label: "transfer.layoutList" },
  { layout: "cards", label: "transfer.layoutCards" },
];

const cards = computed(() => ui.clipboardLayout === "cards");
const projectNames = computed(() =>
  workspace.projects.filter((project) => project.exists).map((project) => project.name),
);
const globalPatterns = computed(() => workspace.config?.transferIgnore ?? []);
const rejected = computed(() => transfer.clipboard.rejected);
const showSource = computed(
  () => transfer.clipboard.source !== "empty" || rejected.value > 0 || transfer.clipboardBusy,
);
const percent = computed(() => (transfer.downloadProgress ? downloadPercent(transfer.downloadProgress) : null));
const progressText = computed(() => {
  const progress = transfer.downloadProgress;
  if (!progress) return "";
  const files = t("transfer.downloadFiles", { done: progress.filesDone, total: progress.filesTotal });
  const bytes =
    progress.bytesTotal === null
      ? formatBytes(progress.bytesDone)
      : t("transfer.downloadBytes", { done: formatBytes(progress.bytesDone), total: formatBytes(progress.bytesTotal) });
  return `${files} · ${bytes}`;
});

function entrySize(entry: ClipboardEntry): string {
  const files = t("transfer.entryFiles", { count: entry.files ?? 0 }, entry.files ?? 0);
  return entry.bytes === null ? files : `${files} · ${formatBytes(entry.bytes)}`;
}
</script>

<template>
  <div class="stack clipboard-step">
    <p class="muted intro">{{ t("transfer.receiveIntro") }}</p>

    <section class="card stack found">
      <div class="card-title">
        <h3>{{ t("transfer.found") }}</h3>
        <span class="row">
          <span class="row muted"><span class="spinner" /> {{ t("transfer.watching") }}</span>
          <span class="segmented">
            <button
              v-for="entry in layouts"
              :key="entry.layout"
              class="chip"
              :class="{ active: ui.clipboardLayout === entry.layout }"
              type="button"
              :aria-pressed="ui.clipboardLayout === entry.layout"
              @click="ui.clipboardLayout = entry.layout"
            >
              {{ t(entry.label) }}
            </button>
          </span>
        </span>
      </div>

      <div v-if="showSource" class="clipboard-source small">
        <p v-if="transfer.clipboard.source === 'paths'">{{ t("transfer.sourcePaths") }}</p>
        <p v-else-if="transfer.clipboard.source === 'virtual' && transfer.download">
          <i18n-t keypath="transfer.sourceDownloaded" tag="span">
            <template #folder
              ><span class="mono selectable">{{ transfer.download.directory }}</span></template
            >
          </i18n-t>
        </p>
        <p v-else-if="transfer.clipboard.source === 'virtual'">{{ t("transfer.sourceRemote") }}</p>
        <p v-else-if="transfer.clipboard.source === 'unreadable'" class="muted selectable">
          {{ transfer.clipboard.problem ?? t("transfer.sourceUnreadable") }}
        </p>
        <p v-if="rejected > 0" class="muted">{{ t("transfer.rejected", { count: rejected }, rejected) }}</p>
        <p v-if="transfer.clipboardBusy" class="muted">{{ t("transfer.sourceBusy") }}</p>
      </div>

      <p v-if="!transfer.entries.length" class="muted">{{ t("transfer.nothing") }}</p>

      <div v-else class="entries" :class="cards ? 'cards' : 'list'">
        <div v-for="entry in transfer.entries" :key="entry.path" class="entry" :class="{ 'list-row': !cards }">
          <AppIcon name="folder" :size="16" />
          <span class="details">
            <span class="row">
              <span class="name">{{ entry.name }}</span>
              <span v-if="entry.files !== null" class="muted small">{{ entrySize(entry) }}</span>
              <span v-if="entry.packageName" class="badge badge-violet mono">{{ entry.packageName }}</span>
              <span v-if="!entry.isProject" class="badge badge-warn">{{ t("transfer.notProject") }}</span>
            </span>
            <span v-if="transfer.folderFor(entry.path)" class="mono muted truncate selectable">{{
              transfer.folderFor(entry.path)
            }}</span>
          </span>
          <label class="target">
            <span v-if="!cards" class="muted small">{{ t("transfer.target") }}</span>
            <select
              class="select"
              :class="{ 'input-sm': cards }"
              :aria-label="cards ? t('transfer.target') : undefined"
              :value="transfer.targets[entry.path] ?? ''"
              :disabled="transfer.downloading"
              @change="transfer.setTarget(entry.path, ($event.target as HTMLSelectElement).value)"
            >
              <option value="">{{ t("transfer.noTarget") }}</option>
              <option v-for="name in projectNames" :key="name" :value="name">{{ name }}</option>
            </select>
          </label>
        </div>
      </div>

      <IgnorePatternChips
        :patterns="globalPatterns"
        :disabled="transfer.receiveDisabled"
        :busy="transfer.receiveBusy"
        @toggle="transfer.toggleReceivePattern($event)"
      />

      <div class="field">
        <label>{{ t("transfer.ignoreRun") }}</label>
        <textarea v-model="transfer.receivePatterns" class="textarea mono" rows="2" placeholder="*.local"></textarea>
      </div>

      <div v-if="transfer.downloadShown" class="download">
        <div class="row">
          <span class="grow">{{ t("transfer.downloading") }}</span>
          <span v-if="progressText" class="muted small">{{ progressText }}</span>
          <ActionButton small icon="close" @click="transfer.cancelDownload()">{{ t("common.cancel") }}</ActionButton>
        </div>
        <ProgressBar class="download-bar" :percent="percent ?? 0" :done="false" :indeterminate="percent === null" />
      </div>
    </section>
  </div>
</template>

<style scoped>
.clipboard-step {
  min-height: 0;
}

.found {
  min-height: 0;
}

.segmented {
  display: inline-flex;
  gap: 4px;
}

.entries {
  min-height: 96px;
  overflow: auto;
}

.entries.cards {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  align-content: start;
  gap: 8px;
}

.entry {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
}

.cards .entry {
  align-items: flex-start;
  gap: 8px;
  padding: 10px 12px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--bg-panel);
}

.cards .details {
  flex: 1 1 0;
}

.cards .details .row {
  flex-wrap: wrap;
  gap: 6px;
}

.cards .target {
  flex: 1 1 100%;
  min-width: 0;
}

.details {
  display: flex;
  flex-direction: column;
  min-width: 0;
  flex: 1 1 260px;
}

.name {
  font-weight: 600;
}

.target {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 220px;
}

.small {
  font-size: 12px;
}

.clipboard-source {
  display: grid;
  gap: 2px;
}

.clipboard-source p {
  margin: 0;
}

.download {
  display: grid;
  gap: 8px;
}

.download .grow {
  flex: 1;
}

/* Short windows: give the folder list room while the controls below it stay in view. */
@media (max-height: 760px) {
  .intro,
  .found :deep(.hint) {
    display: none;
  }

  .entries {
    min-height: 64px;
  }

  .textarea {
    min-height: 34px;
    height: 34px;
  }
}

.download-bar {
  height: 6px;
  border-radius: 3px;
}
</style>
