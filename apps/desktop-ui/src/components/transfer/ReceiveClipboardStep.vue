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
import { useWorkspaceStore } from "@/stores/workspace";
import IgnorePatternChips from "./IgnorePatternChips.vue";

const workspace = useWorkspaceStore();
const transfer = useTransferStore();
const { t } = useI18n();

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
  <div class="stack">
    <p class="muted">{{ t("transfer.receiveIntro") }}</p>

    <section class="card stack">
      <div class="card-title">
        <h3>{{ t("transfer.found") }}</h3>
        <span class="row muted"><span class="spinner" /> {{ t("transfer.watching") }}</span>
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

      <div v-else class="list">
        <div v-for="entry in transfer.entries" :key="entry.path" class="list-row entry">
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
            <span class="muted small">{{ t("transfer.target") }}</span>
            <select
              class="select"
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
.entry {
  flex-wrap: wrap;
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

.download-bar {
  height: 6px;
  border-radius: 3px;
}
</style>
