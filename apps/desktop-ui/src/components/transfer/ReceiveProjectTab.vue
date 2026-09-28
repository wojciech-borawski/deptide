<script setup lang="ts">
import { computed, defineAsyncComponent, ref } from "vue";
import { useI18n } from "vue-i18n";

import type { ReceiveProjectPlan } from "@/api/types";
import ActionButton from "@/components/ui/ActionButton.vue";
import { visibleFiles, type SelectionMode } from "@/lib/receive-view";
import { useTransferStore } from "@/stores/transfer";
import { useUiStore, type ReceiveLayout } from "@/stores/ui";
import ReceiveFileList from "./ReceiveFileList.vue";
import ReceiveFileTree from "./ReceiveFileTree.vue";

const FilePreview = defineAsyncComponent(() => import("./FilePreview.vue"));

const props = defineProps<{ project: ReceiveProjectPlan }>();
const emit = defineEmits<{ replace: [] }>();
const transfer = useTransferStore();
const ui = useUiStore();
const { t } = useI18n();

const selectionModes: { mode: SelectionMode; label: string }[] = [
  { mode: "changes", label: "transfer.selectChanges" },
  { mode: "all", label: "common.all" },
  { mode: "none", label: "common.none" },
];
const layouts: { layout: ReceiveLayout; label: string }[] = [
  { layout: "list", label: "transfer.layoutList" },
  { layout: "tree", label: "transfer.layoutTree" },
];

const files = computed(() => visibleFiles(props.project.files, ui.receiveOnlyAffected));
const count = computed(() => transfer.countFor(props.project.source));
const content = ref<HTMLElement | null>(null);
const filesPane = ref<HTMLElement | null>(null);
const previewed = computed(() => {
  const relative = transfer.previewFor(props.project.source);
  return props.project.files.find((file) => file.relative === relative) ?? null;
});

function closePreview(): void {
  const focused = document.activeElement;
  const focusInPreview = !!focused && !!content.value?.contains(focused) && !filesPane.value?.contains(focused);
  transfer.setPreview(props.project.source, null);
  if (focusInPreview) filesPane.value?.querySelector<HTMLElement>('[role="listbox"], [role="tree"]')?.focus();
}

function setOnlyAffected(event: Event): void {
  const on = (event.target as HTMLInputElement).checked;
  ui.receiveOnlyAffected = on;
  if (on) transfer.untickIdentical();
}
</script>

<template>
  <div class="stack">
    <div class="toolbar">
      <button
        v-for="entry in selectionModes"
        :key="entry.mode"
        class="chip"
        type="button"
        @click="transfer.selectFiles(props.project.source, entry.mode, ui.receiveOnlyAffected)"
      >
        {{ t(entry.label) }}
      </button>
      <span class="divider" />
      <span class="segmented">
        <button
          v-for="entry in layouts"
          :key="entry.layout"
          class="chip"
          :class="{ active: ui.receiveLayout === entry.layout }"
          type="button"
          :aria-pressed="ui.receiveLayout === entry.layout"
          @click="ui.receiveLayout = entry.layout"
        >
          {{ t(entry.label) }}
        </button>
      </span>
      <label class="switch small">
        <input :checked="ui.receiveOnlyAffected" type="checkbox" @change="setOnlyAffected" />
        {{ t("transfer.onlyAffected") }}
      </label>
      <label v-if="ui.receiveLayout === 'list'" class="sort">
        <span class="muted">{{ t("transfer.sort") }}</span>
        <select v-model="ui.receiveSort" class="select input-sm">
          <option value="path">{{ t("transfer.sortByPath") }}</option>
          <option value="change">{{ t("transfer.sortByChange") }}</option>
        </select>
      </label>
      <span class="spacer" />
      <span v-if="props.project.skipped" class="muted small">{{
        t("transfer.skippedByPatterns", { count: props.project.skipped })
      }}</span>
      <ActionButton
        variant="primary"
        small
        :busy="transfer.receiveBusy"
        :disabled="count === 0"
        @click="emit('replace')"
      >
        {{ t("transfer.replaceProject", { count }) }}
      </ActionButton>
    </div>

    <div class="frame">
      <div ref="content" class="content" :class="{ previewing: previewed }">
        <div ref="filesPane" class="files">
          <p v-if="!files.length" class="muted small">{{ t("transfer.nothingVisible") }}</p>
          <ReceiveFileTree v-else-if="ui.receiveLayout === 'tree'" :source="props.project.source" :files="files" />
          <ReceiveFileList v-else :source="props.project.source" :files="files" :sort="ui.receiveSort" />
        </div>
        <FilePreview
          v-if="previewed"
          class="preview"
          :source="props.project.source"
          :file="previewed"
          @close="closePreview"
        />
      </div>
    </div>
  </div>
</template>

<style scoped>
.divider {
  width: 1px;
  height: 18px;
  background: var(--border-strong);
}

.segmented {
  display: inline-flex;
  gap: 4px;
}

.switch.small {
  font-size: 12.5px;
}

.sort {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 12.5px;
}

.sort .select {
  width: auto;
}

.small {
  font-size: 12px;
}

.frame {
  container-type: inline-size;
}

.content {
  display: flex;
  gap: 14px;
  min-height: 0;
}

.files {
  flex: 1 1 0;
  min-width: 0;
  max-height: 460px;
  overflow: auto;
}

.content.previewing {
  height: min(72vh, 760px);
}

.content.previewing .files {
  flex: 0 1 400px;
  max-height: none;
}

.preview {
  flex: 1 1 0;
}

@container (max-width: 1100px) {
  .content.previewing {
    flex-direction: column;
    height: auto;
  }

  .content.previewing .files {
    flex: none;
    max-height: 280px;
  }

  .preview {
    flex: none;
    max-height: 72vh;
  }
}
</style>
