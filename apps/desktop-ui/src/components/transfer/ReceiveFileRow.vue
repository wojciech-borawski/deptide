<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import type { ReceiveFile } from "@/api/types";
import { formatBytes } from "@/lib/format";
import { statusTones } from "@/lib/receive-view";
import type { ChunkChoice } from "@/stores/transfer";

const props = withDefaults(
  defineProps<{
    file: ReceiveFile;
    label: string;
    selected: boolean;
    choice?: ChunkChoice | null;
    previewed?: boolean;
    active?: boolean;
    id?: string;
    depth?: number;
    tree?: boolean;
  }>(),
  { choice: null, previewed: false, active: false, id: undefined, depth: 0, tree: false },
);
const emit = defineEmits<{ toggle: []; open: [] }>();
const { t } = useI18n();
const hintId = computed(() => (props.previewed && props.id ? `${props.id}-hint` : undefined));
const chunks = computed(() =>
  props.choice ? t("transfer.chunkCount", { taken: props.choice.taken.size, total: props.choice.chunks.length }) : null,
);
</script>

<template>
  <div
    :id="props.id"
    class="list-row file"
    :class="{ selected: props.selected, previewed: props.previewed, active: props.active }"
    :style="{ paddingLeft: `${12 + props.depth * 18}px` }"
    :title="props.file.relative"
    :role="props.tree ? 'treeitem' : 'option'"
    :aria-level="props.tree ? props.depth + 1 : undefined"
    :aria-selected="props.previewed"
    :aria-label="[props.label, t(`transfer.statuses.${props.file.status}`), chunks].filter(Boolean).join(', ')"
    :aria-describedby="hintId"
    @click="emit('open')"
  >
    <span v-if="hintId" :id="hintId" hidden>{{ t("transfer.previewedHint") }}</span>
    <span v-if="props.tree" class="spacer-icon" />
    <label class="check" @click.stop>
      <input
        type="checkbox"
        :checked="props.selected"
        :indeterminate="!!props.choice"
        :aria-checked="props.choice ? 'mixed' : undefined"
        :aria-label="props.file.relative"
        @change="emit('toggle')"
      />
    </label>
    <span class="mono truncate path" :class="{ gone: props.file.status === 'removed' }">{{ props.label }}</span>
    <span v-if="chunks" class="muted chunks">{{ chunks }}</span>
    <span class="badge" :class="statusTones[props.file.status]">{{ t(`transfer.statuses.${props.file.status}`) }}</span>
    <span class="mono muted size">{{ formatBytes(props.file.size) }}</span>
  </div>
</template>

<style scoped>
.file {
  cursor: pointer;
  padding: 5px 12px;
  gap: 8px;
}

.file.previewed {
  background: var(--bg-hover);
  box-shadow: inset 3px 0 0 var(--accent);
}

.list:focus-visible .file.active {
  outline: 2px solid var(--accent);
  outline-offset: -2px;
}

.spacer-icon {
  width: 16px;
  flex-shrink: 0;
}

.path {
  flex: 1;
  min-width: 0;
  font-size: 12.5px;
}

.path.gone {
  text-decoration: line-through;
  color: var(--text-muted);
}

.chunks {
  font-size: 11.5px;
  white-space: nowrap;
}

.size {
  font-size: 11.5px;
  min-width: 64px;
  text-align: right;
}
</style>
