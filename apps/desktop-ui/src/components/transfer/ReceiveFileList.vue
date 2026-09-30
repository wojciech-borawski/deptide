<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import type { ReceiveFile } from "@/api/types";
import { useFileCursor } from "@/composables/useFileCursor";
import { comparePaths, groupByChange } from "@/lib/receive-view";
import { useTransferStore } from "@/stores/transfer";
import type { ReceiveSort } from "@/stores/ui";
import ReceiveFileRow from "./ReceiveFileRow.vue";

const props = defineProps<{ source: string; files: ReceiveFile[]; sort: ReceiveSort }>();
const transfer = useTransferStore();
const { t } = useI18n();

const byPath = computed(() => [...props.files].sort((left, right) => comparePaths(left.relative, right.relative)));
const groups = computed(() => groupByChange(props.files));
const order = computed(() =>
  props.sort === "path"
    ? byPath.value.map((file) => file.relative)
    : groups.value.flatMap((group) => group.files.map((file) => file.relative)),
);
const { prefix, list, cursor, activeId, idFor, onKeydown } = useFileCursor(
  () => props.source,
  () => order.value,
);
</script>

<template>
  <div
    ref="list"
    class="list"
    tabindex="0"
    role="listbox"
    :aria-label="t('transfer.planTitle')"
    :aria-activedescendant="activeId"
    @keydown="onKeydown"
  >
    <template v-if="props.sort === 'path'">
      <ReceiveFileRow
        v-for="file in byPath"
        :key="file.relative"
        :file="file"
        :label="file.relative"
        :selected="transfer.isSelected(props.source, file.relative)"
        :choice="transfer.chunkChoiceFor(props.source, file.relative)"
        :id="idFor(file.relative)"
        :previewed="transfer.previewFor(props.source) === file.relative"
        :active="cursor === file.relative"
        @toggle="transfer.toggleFile(props.source, file.relative)"
        @open="transfer.setPreview(props.source, file.relative)"
      />
    </template>
    <template v-else>
      <div
        v-for="group in groups"
        :key="group.status"
        class="group"
        role="group"
        :aria-labelledby="`${prefix}-${group.status}`"
      >
        <div :id="`${prefix}-${group.status}`" class="group-head">
          {{ t(`transfer.statuses.${group.status}`) }}
          <span class="count">{{ group.files.length }}</span>
        </div>
        <ReceiveFileRow
          v-for="file in group.files"
          :key="file.relative"
          :file="file"
          :label="file.relative"
          :selected="transfer.isSelected(props.source, file.relative)"
          :choice="transfer.chunkChoiceFor(props.source, file.relative)"
          :id="idFor(file.relative)"
          :previewed="transfer.previewFor(props.source) === file.relative"
          :active="cursor === file.relative"
          @toggle="transfer.toggleFile(props.source, file.relative)"
          @open="transfer.setPreview(props.source, file.relative)"
        />
      </div>
    </template>
  </div>
</template>

<style scoped>
.group:not(:last-child) .list-row:last-child {
  border-bottom: 1px solid var(--border);
}
</style>
