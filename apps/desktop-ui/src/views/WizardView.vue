<script setup lang="ts">
import { computed, onMounted } from "vue";
import { useEventListener } from "@vueuse/core";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";

import PageHeader from "@/components/layout/PageHeader.vue";
import AppIcon from "@/components/ui/AppIcon.vue";
import StepperHeader from "@/components/ui/StepperHeader.vue";
import StepLibraries from "@/components/wizard/StepLibraries.vue";
import StepOptions from "@/components/wizard/StepOptions.vue";
import StepProjects from "@/components/wizard/StepProjects.vue";
import StepReview from "@/components/wizard/StepReview.vue";
import ActionButton from "@/components/ui/ActionButton.vue";
import { routeNames } from "@/router";
import { useRunStore } from "@/stores/run";
import { useWizardStore, wizardSteps } from "@/stores/wizard";
import { useWorkspaceStore } from "@/stores/workspace";

const workspace = useWorkspaceStore();
const wizard = useWizardStore();
const run = useRunStore();
const router = useRouter();
const { t } = useI18n();

const stepComponents = [StepProjects, StepLibraries, StepOptions, StepReview];

const current = computed(() => stepComponents[wizard.stepIndex] ?? StepProjects);
const startDisabled = computed(() => !wizard.canProceed || run.isActive || run.starting);
const steps = computed(() =>
  wizardSteps.map((key) => ({ title: t(`wizard.steps.${key}.title`), hint: t(`wizard.steps.${key}.hint`) })),
);
const firstProblem = computed(() => {
  const problem = wizard.problems[0];
  if (!problem) return "";
  return problem.name ? `${problem.name}: ${t(problem.key)}` : t(problem.key);
});

async function startRun(): Promise<void> {
  const outcome = await run.start(workspace.root, wizard.plan);
  if (!outcome) return;

  // Leave the wizard before refreshing the workspace: the review step shows a
  // "run in progress" banner as soon as the run starts, which used to flash
  // while the refresh was still awaited. Then hand the wizard back fresh, so
  // "Update" is a new configuration and reruns live in the Run view.
  await router.push({ name: routeNames.run });
  startOver();
  void workspace.refresh();
}

function startOver(): void {
  if (workspace.settings) wizard.resetFromSettings(workspace.settings, workspace.projects);
}

function isTypingTarget(target: EventTarget | null): boolean {
  return target instanceof HTMLTextAreaElement || (target instanceof HTMLElement && target.isContentEditable);
}

function onKeydown(event: KeyboardEvent): void {
  if (isTypingTarget(event.target)) return;

  if (event.key === "Enter" && !event.shiftKey && !event.ctrlKey && !event.metaKey) {
    if (event.target instanceof HTMLButtonElement) return;
    event.preventDefault();
    if (wizard.isLastStep) void startRun();
    else wizard.next();
    return;
  }

  if (event.altKey && event.key === "ArrowLeft") {
    event.preventDefault();
    wizard.back();
  }

  if (event.altKey && event.key === "ArrowRight") {
    event.preventDefault();
    if (!wizard.isLastStep) wizard.next();
  }
}

onMounted(() => {
  if (workspace.settings) wizard.ensureInitialized(workspace.root, workspace.settings, workspace.projects);
});

useEventListener(window, "keydown", onKeydown);
</script>

<template>
  <div class="page">
    <PageHeader :title="t('wizard.title')" :subtitle="workspace.root">
      <button class="btn btn-ghost btn-sm" type="button" @click="startOver">
        <AppIcon name="refresh" :size="14" />
        {{ t("common.startOver") }}
      </button>
    </PageHeader>

    <div class="stepper-bar">
      <StepperHeader :steps="steps" :current="wizard.stepIndex" @select="wizard.goTo" />
    </div>

    <div class="body">
      <transition name="fade" mode="out-in">
        <component :is="current" :key="wizard.stepIndex" />
      </transition>
    </div>

    <footer class="footer">
      <button class="btn" type="button" :disabled="wizard.stepIndex === 0" @click="wizard.back">
        <AppIcon name="chevronLeft" :size="16" />
        {{ t("common.back") }}
      </button>
      <span class="problems">
        <span v-if="run.error" class="notice notice-error selectable">{{ run.error }}</span>
        <span v-else-if="firstProblem" class="muted">{{ firstProblem }}</span>
        <span v-else class="faint">{{ t("wizard.keyboardHint") }}</span>
      </span>
      <button
        v-if="!wizard.isLastStep"
        class="btn btn-primary"
        type="button"
        :disabled="!wizard.canProceed"
        @click="wizard.next"
      >
        {{ t("common.next") }}
        <AppIcon name="chevronRight" :size="16" />
      </button>
      <ActionButton
        v-else
        variant="primary"
        icon="play"
        :busy="run.starting"
        :disabled="startDisabled"
        @click="startRun"
      >
        {{ t("wizard.startRun") }}
      </ActionButton>
    </footer>
  </div>
</template>

<style scoped>
.page {
  display: flex;
  flex-direction: column;
  height: 100%;
}

.stepper-bar {
  padding: 14px 24px;
  border-bottom: 1px solid var(--border);
  background: var(--bg-app);
}

.body {
  flex: 1;
  overflow: auto;
  padding: 20px 24px;
}

.footer {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 12px 24px;
  border-top: 1px solid var(--border);
  background: var(--bg-panel);
}

.problems {
  flex: 1;
  font-size: 13px;
  min-width: 0;
}
</style>
