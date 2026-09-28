<script setup lang="ts">
import { useI18n } from "vue-i18n";

import ActionButton from "@/components/ui/ActionButton.vue";
import { formatBytes, formatDateTime } from "@/lib/format";
import type { ReceivedProject } from "@/stores/transfer";

const props = defineProps<{ received: ReceivedProject }>();
const emit = defineEmits<{ close: [] }>();
const { t } = useI18n();
</script>

<template>
  <div class="stack">
    <div class="toolbar">
      <h3 class="ok">{{ t("transfer.resultTitle", { target: props.received.result.target }) }}</h3>
      <span class="muted"
        >{{ formatDateTime(props.received.receivedAt) }} · {{ formatBytes(props.received.result.bytes) }}</span
      >
      <span class="spacer" />
      <ActionButton small icon="close" @click="emit('close')">{{ t("common.close") }}</ActionButton>
    </div>
    <p class="counts">
      {{
        t("transfer.resultCounts", {
          added: props.received.result.added,
          replaced: props.received.result.replaced,
          deleted: props.received.result.deleted,
          recycled: props.received.result.recycled,
        })
      }}
    </p>

    <section v-if="props.received.result.skipped.length" class="block">
      <h4 class="warn">{{ t("transfer.resultSkipped") }} ({{ props.received.result.skipped.length }})</h4>
      <ul class="mono selectable">
        <li v-for="path in props.received.result.skipped" :key="path">{{ path }}</li>
      </ul>
    </section>
    <section v-if="props.received.result.files.length" class="block">
      <h4>{{ t("transfer.resultCopied") }} ({{ props.received.result.files.length }})</h4>
      <ul class="mono selectable">
        <li v-for="path in props.received.result.files" :key="path">{{ path }}</li>
      </ul>
    </section>
    <section v-if="props.received.result.deletedFiles.length" class="block">
      <h4>{{ t("transfer.resultDeleted") }} ({{ props.received.result.deletedFiles.length }})</h4>
      <ul class="mono selectable">
        <li v-for="path in props.received.result.deletedFiles" :key="path">{{ path }}</li>
      </ul>
    </section>

    <p v-if="props.received.logFile" class="muted small selectable">
      {{ t("transfer.logWritten", { path: props.received.logFile }) }}
    </p>
  </div>
</template>

<style scoped>
h3 {
  margin: 0;
}

.ok {
  color: var(--ok);
  text-transform: none;
  font-size: 15px;
  letter-spacing: 0;
}

.counts {
  margin: 0;
}

h4 {
  margin: 0 0 6px;
  font-size: 12.5px;
  color: var(--text-muted);
}

h4.warn {
  color: var(--warn);
}

ul {
  margin: 0;
  padding-left: 18px;
  display: grid;
  gap: 2px;
  max-height: 240px;
  overflow: auto;
}

.small {
  font-size: 12px;
  margin: 0;
}
</style>
