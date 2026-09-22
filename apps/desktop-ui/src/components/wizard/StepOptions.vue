<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import type { ExecutionMode, StepName } from "@/api/types";
import { allBumps, allSteps, forceSteps, fullSteps, simpleSteps } from "@/api/types";
import { orderedSteps, parseArguments } from "@/lib/wizard-plan";
import { useWizardStore } from "@/stores/wizard";

const wizard = useWizardStore();
const { t } = useI18n();

const modes: Array<{ value: ExecutionMode; title: string; text: string }> = [
  { value: "per-project", title: "options.perProject", text: "options.perProjectText" },
  { value: "per-step", title: "options.perStep", text: "options.perStepText" },
];

const presets = [
  { key: "full", steps: fullSteps },
  { key: "simple", steps: simpleSteps },
  { key: "force", steps: forceSteps },
];

const concurrencyPresets = [1, 3, 5, 8];

const installFlags = ["--force", "--save-exact", "--legacy-peer-deps", "--no-fund", "--prefer-offline"];

const activePreset = computed(
  () =>
    presets.find(
      (preset) =>
        preset.steps.length === wizard.draft.steps.length &&
        preset.steps.every((step) => wizard.draft.steps.includes(step)),
    )?.key ?? null,
);

const activeFlags = computed(() => new Set(parseArguments(wizard.draft.extraInstallArgs)));

function hasStep(step: StepName): boolean {
  return wizard.draft.steps.includes(step);
}

function toggleStep(step: StepName): void {
  const next = hasStep(step) ? wizard.draft.steps.filter((entry) => entry !== step) : [...wizard.draft.steps, step];
  wizard.draft.steps = orderedSteps(next);
}

function applyPreset(steps: readonly StepName[]): void {
  wizard.draft.steps = [...steps];
}

function toggleFlag(flag: string): void {
  const flags = parseArguments(wizard.draft.extraInstallArgs);
  const next = flags.includes(flag) ? flags.filter((entry) => entry !== flag) : [...flags, flag];
  wizard.draft.extraInstallArgs = next.join(" ");
}
</script>

<template>
  <div class="stack">
    <section class="card">
      <div class="card-title">
        <h3>{{ t("options.stepsTitle") }}</h3>
        <div class="row">
          <button
            v-for="preset in presets"
            :key="preset.key"
            class="chip"
            :class="{ active: activePreset === preset.key }"
            type="button"
            @click="applyPreset(preset.steps)"
          >
            {{ t(`options.presets.${preset.key}`) }}
          </button>
        </div>
      </div>
      <div class="steps">
        <label v-for="(step, index) in allSteps" :key="step" class="step-card" :class="{ active: hasStep(step) }">
          <span class="check">
            <input type="checkbox" :checked="hasStep(step)" @change="toggleStep(step)" />
          </span>
          <span class="step-text">
            <span class="step-title"
              ><span class="faint">{{ index + 1 }}</span> {{ t(`steps.${step}`) }}</span
            >
            <span class="muted">{{ t(`options.stepText.${step}`) }}</span>
          </span>
        </label>
      </div>

      <div v-if="hasStep('version')" class="sub-panel stack">
        <div class="field">
          <label>{{ t("options.bump.title") }}</label>
          <div class="row">
            <button
              v-for="bump in allBumps"
              :key="bump"
              class="chip"
              :class="{ active: wizard.draft.versionBump === bump }"
              type="button"
              @click="wizard.draft.versionBump = bump"
            >
              {{ t(`options.bump.${bump}`) }}
            </button>
          </div>
          <span class="hint muted">{{ t("options.bump.hint") }}</span>
        </div>
        <label class="switch">
          <input v-model="wizard.draft.bumpOnlyIfSameAsMain" type="checkbox" />
          <span>{{ t("options.bump.onlyIfSameAsMain") }}</span>
        </label>
        <span class="hint muted">{{ t("options.bump.onlyIfSameAsMainHint") }}</span>
      </div>
    </section>

    <div class="grid-2">
      <section class="card">
        <div class="card-title">
          <h3>{{ t("options.order") }}</h3>
        </div>
        <div class="stack modes">
          <label
            v-for="mode in modes"
            :key="mode.value"
            class="mode"
            :class="{ active: wizard.draft.mode === mode.value }"
          >
            <span class="check">
              <input v-model="wizard.draft.mode" type="radio" :value="mode.value" />
            </span>
            <span class="step-text">
              <span class="step-title">{{ t(mode.title) }}</span>
              <span class="muted">{{ t(mode.text) }}</span>
            </span>
          </label>
        </div>
      </section>

      <section class="card stack">
        <div class="card-title">
          <h3>{{ t("options.parallelism") }}</h3>
        </div>
        <div class="field">
          <label>{{ t("options.atTheSameTime") }}</label>
          <div class="row">
            <button
              v-for="preset in concurrencyPresets"
              :key="preset"
              class="chip"
              :class="{ active: wizard.draft.concurrency === preset }"
              type="button"
              @click="wizard.draft.concurrency = preset"
            >
              {{ preset }}
            </button>
            <input
              v-model.number="wizard.draft.concurrency"
              class="input input-sm number"
              type="number"
              min="1"
              max="32"
            />
          </div>
          <span class="hint">{{ t("options.parallelHint") }}</span>
        </div>

        <label class="switch">
          <input v-model="wizard.draft.dryRun" type="checkbox" />
          <span>{{ t("options.dryRun") }}</span>
        </label>
      </section>
    </div>

    <section class="card stack">
      <div class="card-title">
        <h3>{{ t("options.flags") }}</h3>
      </div>
      <div class="row wrap">
        <button
          v-for="flag in installFlags"
          :key="flag"
          class="chip mono"
          :class="{ active: activeFlags.has(flag) }"
          type="button"
          @click="toggleFlag(flag)"
        >
          {{ flag }}
        </button>
      </div>
      <input v-model="wizard.draft.extraInstallArgs" class="input mono" placeholder="--force --legacy-peer-deps" />
      <span class="hint muted">{{ t("options.flagsHint") }}</span>
    </section>
  </div>
</template>

<style scoped>
.steps {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 10px;
}

.step-card,
.mode {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 12px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  cursor: pointer;
  background: var(--bg-elevated);
  transition:
    border-color 0.12s ease,
    background 0.12s ease;
}

.step-card:hover,
.mode:hover {
  border-color: var(--border-strong);
}

.step-card.active,
.mode.active {
  border-color: var(--accent);
  background: var(--accent-soft);
}

.step-card .check,
.mode .check {
  margin-top: 2px;
}

.step-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
  font-size: 13px;
}

.step-title {
  font-weight: 600;
}

.modes {
  gap: 10px;
}

.number {
  width: 80px;
}

.wrap {
  flex-wrap: wrap;
}

.sub-panel {
  margin-top: 12px;
  padding: 12px;
  border: 1px dashed var(--border);
  border-radius: var(--radius-sm);
}
</style>
