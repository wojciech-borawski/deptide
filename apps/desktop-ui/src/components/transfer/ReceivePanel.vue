<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useEventListener } from "@vueuse/core";
import { useI18n } from "vue-i18n";

import AppIcon from "@/components/ui/AppIcon.vue";
import ActionButton from "@/components/ui/ActionButton.vue";
import NoticeBanner from "@/components/ui/NoticeBanner.vue";
import StepperHeader from "@/components/ui/StepperHeader.vue";
import type { RemovalGroup } from "@/lib/receive-view";
import { useTransferStore, type ReceiveStep } from "@/stores/transfer";
import { useWorkspaceStore } from "@/stores/workspace";
import DeleteConfirmDialog from "./DeleteConfirmDialog.vue";
import DiscardSessionDialog from "./DiscardSessionDialog.vue";
import ReceiveClipboardStep from "./ReceiveClipboardStep.vue";
import ReceiveReviewStep from "./ReceiveReviewStep.vue";

interface PendingReplace {
  source: string | undefined;
  groups: RemovalGroup[];
}

const receiveSteps: ReceiveStep[] = ["clipboard", "review"];

const workspace = useWorkspaceStore();
const transfer = useTransferStore();
const { t } = useI18n();

const pending = ref<PendingReplace | null>(null);
const confirmingDiscard = ref(false);

const steps = computed(() =>
  receiveSteps.map((key) => ({
    title: t(`transfer.receiveSteps.${key}.title`),
    hint: t(`transfer.receiveSteps.${key}.hint`),
  })),
);
const stepIndex = computed(() => receiveSteps.indexOf(transfer.receiveStep));
const onClipboardStep = computed(() => transfer.receiveStep === "clipboard");
const canAnalyze = computed(() => transfer.requests.length > 0 && !transfer.receiveBusy);
const allDone = computed(() => transfer.hasSession && transfer.pendingCount === 0);

function selectStep(index: number): void {
  const step = receiveSteps[index];
  if (step) transfer.setReceiveStep(step);
}

function analyze(): void {
  if (transfer.clipboardChanged && transfer.pendingCount > 0) confirmingDiscard.value = true;
  else void transfer.analyze(workspace.root);
}

function confirmDiscard(): void {
  confirmingDiscard.value = false;
  void transfer.analyze(workspace.root);
}

function replace(source?: string): void {
  const groups = transfer.removalsFor(source);
  if (groups.length) pending.value = { source, groups };
  else void transfer.apply(workspace.root, source);
}

function confirmReplace(): void {
  const source = pending.value?.source;
  pending.value = null;
  void transfer.apply(workspace.root, source);
}

function isTypingTarget(target: EventTarget | null): boolean {
  const element = target as HTMLElement | null;
  return element?.tagName === "TEXTAREA" || element?.isContentEditable === true;
}

function onKeydown(event: KeyboardEvent): void {
  if (!event.altKey || isTypingTarget(event.target)) return;
  if (event.key === "ArrowLeft") {
    event.preventDefault();
    transfer.setReceiveStep("clipboard");
  } else if (event.key === "ArrowRight") {
    event.preventDefault();
    transfer.setReceiveStep("review");
  }
}

onMounted(() => transfer.startWatching(workspace.root));
onBeforeUnmount(() => transfer.stopWatching());
useEventListener(window, "keydown", onKeydown);
</script>

<template>
  <div class="receive">
    <div class="stepper-bar">
      <StepperHeader
        :steps="steps"
        :current="stepIndex"
        :reachable="transfer.hasSession ? receiveSteps.length - 1 : -1"
        @select="selectStep"
      />
    </div>

    <NoticeBanner v-if="transfer.receiveError" tone="error" selectable>{{ transfer.receiveError }}</NoticeBanner>

    <div class="step">
      <ReceiveClipboardStep v-if="onClipboardStep" />
      <ReceiveReviewStep v-else @replace="replace" />
    </div>

    <footer class="footer">
      <button class="btn" type="button" :disabled="onClipboardStep" @click="transfer.setReceiveStep('clipboard')">
        <AppIcon name="chevronLeft" :size="16" />
        {{ t("common.back") }}
      </button>
      <span class="hint faint">{{ t("transfer.receiveKeyboardHint") }}</span>
      <ActionButton
        v-if="onClipboardStep"
        variant="primary"
        icon="search"
        :busy="transfer.receiveBusy"
        :disabled="!canAnalyze"
        @click="analyze"
      >
        {{ t("transfer.analyze") }}
      </ActionButton>
      <ActionButton v-else-if="allDone" variant="primary" icon="refresh" @click="transfer.endSession()">
        {{ t("transfer.receiveMore") }}
      </ActionButton>
    </footer>

    <DiscardSessionDialog
      :open="confirmingDiscard"
      :count="transfer.pendingCount"
      @cancel="confirmingDiscard = false"
      @confirm="confirmDiscard"
    />
    <DeleteConfirmDialog
      :open="pending !== null"
      :groups="pending?.groups ?? []"
      @cancel="pending = null"
      @confirm="confirmReplace"
    />
  </div>
</template>

<style scoped>
.receive {
  display: flex;
  flex-direction: column;
  gap: 14px;
  flex: 1;
  min-height: 0;
}

.stepper-bar {
  padding-bottom: 14px;
  border-bottom: 1px solid var(--border);
}

.step {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
  overflow: auto;
}

.footer {
  display: flex;
  align-items: center;
  gap: 14px;
  padding-top: 12px;
  border-top: 1px solid var(--border);
}

.hint {
  flex: 1;
  font-size: 13px;
  min-width: 0;
}
</style>
