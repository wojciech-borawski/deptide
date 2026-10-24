<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import type { AuditCounts } from "@/api/types";
import { auditBadges, type AuditSeverity } from "@/lib/audit";

const props = defineProps<{ counts: AuditCounts }>();
const { t } = useI18n();

const badges = computed(() => auditBadges(props.counts));
const title = computed(() =>
  t("audit.title", {
    details: badges.value.length
      ? badges.value.map((badge) => `${badge.count} ${t(`audit.${badge.severity}`)}`).join(", ")
      : t("audit.none"),
  }),
);

const tone: Record<AuditSeverity, string> = {
  critical: "badge-failed",
  high: "badge-failed",
  moderate: "badge-warn",
  low: "badge-skipped",
  info: "badge-skipped",
};
</script>

<template>
  <span class="row wrap audit" :title="title">
    <span v-if="badges.length === 0" class="badge badge-ok small">{{ t("audit.none") }}</span>
    <span v-for="badge in badges" :key="badge.severity" class="badge small" :class="tone[badge.severity]">
      {{ badge.count }} {{ t(`audit.${badge.severity}`) }}
    </span>
  </span>
</template>

<style scoped>
.audit {
  gap: 4px;
}
</style>
