<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";

import type { JobSnapshot, StepName } from "@/api/types";
import { allSteps } from "@/api/types";
import ModalDialog from "@/components/ui/ModalDialog.vue";
import StatusBadge from "@/components/ui/StatusBadge.vue";
import ActionButton from "@/components/ui/ActionButton.vue";
import { useCountedNoun } from "@/composables/useCountedNoun";
import { orderedSteps } from "@/lib/wizard-plan";

const props = defineProps<{
  open: boolean;
  jobs: JobSnapshot[];
  steps: StepName[];
  preselected: string[];
  busy: boolean;
}>();
const emit = defineEmits<{ close: []; start: [projectNames: string[], steps: StepName[]] }>();
const { t } = useI18n();
const countOf = useCountedNoun();

const chosenProjects = ref<Set<string>>(new Set());
const chosenSteps = ref<StepName[]>([]);

const canStart = computed(() => chosenProjects.value.size > 0 && chosenSteps.value.length > 0);

function toggleProject(name: string): void {
  const next = new Set(chosenProjects.value);
  if (next.has(name)) next.delete(name);
  else next.add(name);
  chosenProjects.value = next;
}

function selectByStatus(statuses: JobSnapshot["status"][]): void {
  chosenProjects.value = new Set(props.jobs.filter((job) => statuses.includes(job.status)).map((job) => job.name));
}

function hasStep(step: StepName): boolean {
  return chosenSteps.value.includes(step);
}

function toggleStep(step: StepName): void {
  const next = hasStep(step) ? chosenSteps.value.filter((entry) => entry !== step) : [...chosenSteps.value, step];
  chosenSteps.value = orderedSteps(next);
}

function start(): void {
  if (!canStart.value) return;
  emit(
    "start",
    props.jobs.filter((job) => chosenProjects.value.has(job.name)).map((job) => job.name),
    chosenSteps.value,
  );
}

watch(
  () => props.open,
  (open) => {
    if (!open) return;
    chosenProjects.value = new Set(props.preselected);
    chosenSteps.value = [...props.steps];
  },
);
</script>

<template>
  <ModalDialog :open="props.open" :title="t('run.rerun.title')" width="640px" @close="emit('close')">
    <div class="stack">
      <div class="toolbar">
        <span class="muted">{{ t("run.rerun.hint") }}</span>
        <span class="spacer" />
        <button class="btn btn-sm" type="button" @click="selectByStatus(['failed', 'skipped'])">
          {{ t("run.rerun.failedAndSkipped") }}
        </button>
        <button
          class="btn btn-sm"
          type="button"
          @click="selectByStatus(['ok', 'warn', 'failed', 'skipped', 'pending', 'running'])"
        >
          {{ t("common.all") }}
        </button>
        <button class="btn btn-sm" type="button" @click="chosenProjects = new Set()">{{ t("common.none") }}</button>
      </div>

      <div class="list scroll">
        <label
          v-for="job in props.jobs"
          :key="job.name"
          class="list-row pick"
          :class="{ selected: chosenProjects.has(job.name) }"
        >
          <span class="check">
            <input type="checkbox" :checked="chosenProjects.has(job.name)" @change="toggleProject(job.name)" />
          </span>
          <span class="name">{{ job.name }}</span>
          <span class="muted truncate detail">{{ job.error || job.warning }}</span>
          <StatusBadge :status="job.status" />
        </label>
      </div>

      <div class="field">
        <label>{{ t("run.rerun.stepsToRun") }}</label>
        <div class="row wrap">
          <label v-for="step in allSteps" :key="step" class="check">
            <input type="checkbox" :checked="hasStep(step)" @change="toggleStep(step)" />
            <span>{{ t(`steps.${step}`) }}</span>
          </label>
        </div>
        <span class="hint">{{ t("run.rerun.stepsHint") }}</span>
      </div>
    </div>

    <template #footer>
      <button class="btn" type="button" @click="emit('close')">{{ t("common.cancel") }}</button>
      <ActionButton variant="primary" :busy="props.busy" :disabled="!canStart" @click="start">
        {{ t("run.rerun.start", { count: countOf("project", chosenProjects.size) }) }}
      </ActionButton>
    </template>
  </ModalDialog>
</template>

<style scoped>
.scroll {
  max-height: 40vh;
  overflow: auto;
}

.pick {
  cursor: pointer;
}

.name {
  font-weight: 600;
  min-width: 180px;
}

.detail {
  flex: 1;
  font-size: 12px;
}

.wrap {
  flex-wrap: wrap;
  gap: 14px;
}
</style>
