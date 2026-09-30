<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import AppIcon from "@/components/ui/AppIcon.vue";
import ActionButton from "@/components/ui/ActionButton.vue";
import { useTransferStore } from "@/stores/transfer";
import ReceiveProjectTab from "./ReceiveProjectTab.vue";
import ReceiveResult from "./ReceiveResult.vue";
import ReceiveTabs from "./ReceiveTabs.vue";

const emit = defineEmits<{ replace: [source: string | undefined] }>();
const transfer = useTransferStore();
const { t } = useI18n();

const activeProject = computed(() => transfer.tabs.find((project) => project.source === transfer.activeSource));
const activeResult = computed(() => (transfer.activeSource ? transfer.results[transfer.activeSource] : undefined));
const doneSources = computed(() => Object.keys(transfer.results));

function closeActive(): void {
  if (transfer.activeSource) transfer.closeProject(transfer.activeSource);
}
</script>

<template>
  <div class="review">
    <p v-if="transfer.clipboardChanged" class="changed small">
      <span class="muted">{{ t("transfer.clipboardChanged") }}</span>
      <button class="btn btn-ghost btn-sm" type="button" @click="transfer.setReceiveStep('clipboard')">
        <AppIcon name="chevronLeft" :size="14" />
        {{ t("transfer.backToClipboard") }}
      </button>
    </p>

    <section class="card plan">
      <div class="card-title">
        <h3>{{ t("transfer.planTitle") }}</h3>
        <ActionButton
          v-if="transfer.tabs.length"
          variant="primary"
          small
          :busy="transfer.receiveBusy"
          :disabled="transfer.selectedCount === 0"
          @click="emit('replace', undefined)"
        >
          {{ t("transfer.replaceAll", { count: transfer.selectedCount }) }}
        </ActionButton>
      </div>

      <details v-if="transfer.unchangedProjects.length" class="unchanged">
        <summary>
          <AppIcon name="chevronRight" :size="14" class="chevron" />
          {{ t("transfer.noChanges", { projects: t("common.project", transfer.unchangedProjects.length) }) }}
        </summary>
        <ul>
          <li v-for="project in transfer.unchangedProjects" :key="project.source">
            <strong>{{ project.target }}</strong>
            <span class="mono muted small source">{{ project.source }}</span>
          </li>
        </ul>
      </details>

      <ReceiveTabs
        v-if="transfer.tabs.length"
        :projects="transfer.tabs"
        :active="transfer.activeSource"
        :done="doneSources"
        @select="transfer.setActive"
        @close="transfer.closeProject"
      >
        <ReceiveResult v-if="activeResult" :received="activeResult" @close="closeActive" />
        <ReceiveProjectTab
          v-else-if="activeProject"
          :key="activeProject.source"
          :project="activeProject"
          @replace="emit('replace', activeProject.source)"
        />
      </ReceiveTabs>
    </section>
  </div>
</template>

<style scoped>
.review {
  display: flex;
  flex-direction: column;
  gap: 14px;
  flex: 1;
  min-height: 0;
}

.changed {
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 0;
}

.small {
  font-size: 12px;
}

.plan {
  display: flex;
  flex-direction: column;
  gap: 14px;
  flex: 1;
  min-height: 0;
}

.plan .card-title {
  margin-bottom: 0;
}

.unchanged summary {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  cursor: pointer;
  color: var(--text-muted);
  font-size: 13px;
  user-select: none;
  list-style: none;
}

.unchanged summary::-webkit-details-marker {
  display: none;
}

.unchanged .chevron {
  transition: transform 0.1s ease;
}

.unchanged[open] .chevron {
  transform: rotate(90deg);
}

.unchanged .source {
  margin-left: 8px;
}

.unchanged ul {
  margin: 8px 0 0;
  padding-left: 26px;
  display: grid;
  gap: 4px;
  font-size: 13px;
}
</style>
