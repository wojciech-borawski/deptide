<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import type { ReceiveFile } from "@/api/types";
import AppIcon from "@/components/ui/AppIcon.vue";
import { useFileCursor } from "@/composables/useFileCursor";
import { buildTree, checkState, flattenTree, type TreeFolder } from "@/lib/receive-view";
import { useTransferStore } from "@/stores/transfer";
import ReceiveFileRow from "./ReceiveFileRow.vue";
import StatusPills from "./StatusPills.vue";

const props = defineProps<{ source: string; files: ReceiveFile[] }>();
const transfer = useTransferStore();
const { t } = useI18n();

const tree = computed(() => buildTree(props.files));
const rows = computed(() => flattenTree(tree.value, (path) => transfer.isExpanded(props.source, path)));
const order = computed(() => rows.value.map((row) => row.node.path));
const folders = computed(
  () =>
    new Map(
      rows.value.flatMap((row) =>
        row.node.kind === "folder" ? [[row.node.path, transfer.isExpanded(props.source, row.node.path)] as const] : [],
      ),
    ),
);
const { list, cursor, activeId, idFor, onKeydown } = useFileCursor(
  () => props.source,
  () => order.value,
  () => folders.value,
);

function folderState(folder: TreeFolder) {
  return checkState(folder.files, transfer.selectedFor(props.source));
}

function toggleFolderSelection(folder: TreeFolder): void {
  transfer.setFiles(props.source, folder.files, folderState(folder) !== "checked");
}
</script>

<template>
  <div
    ref="list"
    class="list"
    tabindex="0"
    role="tree"
    :aria-label="t('transfer.planTitle')"
    :aria-activedescendant="activeId"
    @keydown="onKeydown"
  >
    <template v-for="row in rows" :key="row.node.path">
      <div
        v-if="row.node.kind === 'folder'"
        :id="idFor(row.node.path)"
        class="list-row folder"
        :class="{ active: cursor === row.node.path }"
        :style="{ paddingLeft: `${12 + row.depth * 18}px` }"
        role="treeitem"
        :aria-label="row.node.name"
        :aria-level="row.depth + 1"
        :aria-expanded="transfer.isExpanded(props.source, row.node.path)"
      >
        <button
          class="chevron"
          :class="{ open: transfer.isExpanded(props.source, row.node.path) }"
          type="button"
          :aria-expanded="transfer.isExpanded(props.source, row.node.path)"
          :aria-label="
            transfer.isExpanded(props.source, row.node.path)
              ? t('transfer.collapseFolder', { name: row.node.name })
              : t('transfer.expandFolder', { name: row.node.name })
          "
          @click="transfer.toggleFolder(props.source, row.node.path)"
        >
          <AppIcon name="chevronRight" :size="14" />
        </button>
        <label class="check">
          <input
            type="checkbox"
            :checked="folderState(row.node) === 'checked'"
            :indeterminate="folderState(row.node) === 'indeterminate'"
            :aria-label="row.node.path"
            @change="toggleFolderSelection(row.node)"
          />
        </label>
        <AppIcon name="folder" :size="15" class="folder-icon" />
        <span class="mono truncate name" @click="transfer.toggleFolder(props.source, row.node.path)">{{
          row.node.name
        }}</span>
        <StatusPills :counts="row.node.counts" />
      </div>
      <ReceiveFileRow
        v-else
        :file="row.node.file"
        :label="row.node.name"
        :depth="row.depth"
        tree
        :selected="transfer.isSelected(props.source, row.node.path)"
        :id="idFor(row.node.path)"
        :previewed="transfer.previewFor(props.source) === row.node.path"
        :active="cursor === row.node.path"
        @toggle="transfer.toggleFile(props.source, row.node.path)"
        @open="transfer.setPreview(props.source, row.node.path)"
      />
    </template>
  </div>
</template>

<style scoped>
.folder {
  padding-top: 5px;
  padding-bottom: 5px;
  gap: 8px;
}

.list:focus-visible .folder.active {
  outline: 2px solid var(--accent);
  outline-offset: -2px;
}

.chevron {
  display: grid;
  place-items: center;
  width: 16px;
  height: 16px;
  padding: 0;
  border: none;
  background: none;
  color: var(--text-muted);
  cursor: pointer;
  flex-shrink: 0;
  transition: transform 0.1s ease;
}

.chevron.open {
  transform: rotate(90deg);
}

.folder-icon {
  color: var(--text-muted);
  flex-shrink: 0;
}

.name {
  flex: 1;
  min-width: 0;
  font-size: 12.5px;
  cursor: pointer;
}
</style>
