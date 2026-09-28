<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import type { StatusCounts } from "@/lib/receive-view";

type PillStatus = "added" | "replaced" | "removed" | "whitespace";

const props = defineProps<{ counts: Pick<StatusCounts, PillStatus> }>();
const { t } = useI18n();

const pills: { status: PillStatus; sign: string; tone: string }[] = [
  { status: "added", sign: "+", tone: "badge-ok" },
  { status: "replaced", sign: "~", tone: "badge-warn" },
  { status: "removed", sign: "−", tone: "badge-failed" },
  { status: "whitespace", sign: "≈", tone: "badge-skipped" },
];

const visible = computed(() => pills.filter((pill) => props.counts[pill.status] > 0));
</script>

<template>
  <span class="pills">
    <span
      v-for="pill in visible"
      :key="pill.status"
      class="badge pill mono"
      :class="pill.tone"
      :title="t(`transfer.pills.${pill.status}`, { count: props.counts[pill.status] })"
      >{{ pill.sign }}{{ props.counts[pill.status] }}</span
    >
  </span>
</template>

<style scoped>
.pills {
  display: inline-flex;
  gap: 4px;
}

.pill {
  padding: 1px 6px;
  font-size: 11px;
}
</style>
