<script setup lang="ts">
import { useI18n } from "vue-i18n";

import PageHeader from "@/components/layout/PageHeader.vue";
import CopyPanel from "@/components/transfer/CopyPanel.vue";
import ReceivePanel from "@/components/transfer/ReceivePanel.vue";
import { useTransferStore, type TransferTab } from "@/stores/transfer";
import { useWorkspaceStore } from "@/stores/workspace";

const workspace = useWorkspaceStore();
const transfer = useTransferStore();
const { t } = useI18n();

const tabs: TransferTab[] = ["copy", "receive"];
</script>

<template>
  <div class="page">
    <PageHeader :title="t('transfer.title')" :subtitle="workspace.root">
      <div class="tabs">
        <button
          v-for="tab in tabs"
          :key="tab"
          class="tab"
          :class="{ active: transfer.tab === tab }"
          type="button"
          @click="transfer.tab = tab"
        >
          {{ tab === "copy" ? t("transfer.copyTab") : t("transfer.receiveTab") }}
        </button>
      </div>
    </PageHeader>

    <div class="body">
      <CopyPanel v-if="transfer.tab === 'copy'" />
      <ReceivePanel v-else />
    </div>
  </div>
</template>

<style scoped>
.page {
  display: flex;
  flex-direction: column;
  height: 100%;
}

.body {
  flex: 1;
  overflow: auto;
  padding: 20px 24px;
}

.tabs {
  display: inline-flex;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  overflow: hidden;
}

.tab {
  border: none;
  background: var(--bg-elevated);
  color: var(--text-muted);
  font: inherit;
  font-weight: 600;
  padding: 7px 16px;
  cursor: pointer;
}

.tab.active {
  background: var(--accent-soft);
  color: var(--accent);
}
</style>
