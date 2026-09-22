<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import NoticeBanner from "@/components/ui/NoticeBanner.vue";
import { useCountedNoun } from "@/composables/useCountedNoun";

import { formatSpec } from "@/lib/versions";
import { useRunStore } from "@/stores/run";
import { useWizardStore } from "@/stores/wizard";

const wizard = useWizardStore();
const run = useRunStore();
const { t } = useI18n();
const countOf = useCountedNoun();

const plan = computed(() => wizard.plan);
const orderLabel = computed(() =>
  plan.value.mode === "per-step" ? t("common.stepByStep") : t("common.projectByProject"),
);
</script>

<template>
  <div class="stack">
    <NoticeBanner v-if="run.isActive" tone="warn">{{ t("review.runInProgress") }}</NoticeBanner>

    <div class="grid-2">
      <section class="card">
        <div class="card-title">
          <h3>{{ t("review.projects") }}</h3>
          <span class="badge badge-accent">{{ plan.projectNames.length }}</span>
        </div>
        <div class="chips">
          <span v-for="name in plan.projectNames" :key="name" class="chip">{{ name }}</span>
        </div>
      </section>

      <section class="card">
        <div class="card-title">
          <h3>{{ t("review.libraries") }}</h3>
          <span class="badge badge-accent">{{ plan.packages.length }}</span>
        </div>
        <div class="chips">
          <span v-for="spec in plan.packages" :key="spec.name" class="chip mono active">
            {{ formatSpec(spec) }}<span v-if="spec.saveDev" class="faint"> dev</span>
          </span>
        </div>
        <p class="hint muted">{{ t("review.librariesHint") }}</p>
      </section>
    </div>

    <section class="card">
      <div class="card-title">
        <h3>{{ t("review.execution") }}</h3>
      </div>
      <dl class="facts">
        <dt>{{ t("review.steps") }}</dt>
        <dd class="row">
          <span v-for="step in plan.steps" :key="step" class="badge badge-violet">{{ t(`steps.${step}`) }}</span>
        </dd>
        <dt>{{ t("review.order") }}</dt>
        <dd>{{ orderLabel }}</dd>
        <dt>{{ t("review.parallel") }}</dt>
        <dd>
          {{ t("review.atATime", { count: countOf("project", plan.concurrency) }) }}
        </dd>
        <dt>{{ t("review.installFlags") }}</dt>
        <dd class="mono">{{ plan.extraInstallArgs.join(" ") || "-" }}</dd>
        <template v-if="plan.steps.includes('version')">
          <dt>{{ t("review.versionBump") }}</dt>
          <dd>
            {{ t(`options.bump.${plan.version.bump}`) }}
            <span v-if="plan.version.onlyIfSameAsMain" class="muted"> · {{ t("review.onlyIfSameAsMain") }}</span>
          </dd>
        </template>
        <dt>{{ t("review.mode") }}</dt>
        <dd>
          <span v-if="plan.dryRun" class="badge badge-warn">{{ t("common.dryRun") }}</span>
          <span v-else class="muted">{{ t("common.realRun") }}</span>
        </dd>
      </dl>
    </section>

    <section class="card save-grid">
      <div class="field">
        <label for="run-label">{{ t("review.label") }}</label>
        <input
          id="run-label"
          class="input"
          :value="wizard.draft.label"
          @input="wizard.setLabel(($event.target as HTMLInputElement).value)"
        />
        <span class="hint">{{ t("review.labelHint") }}</span>
      </div>
      <div class="field">
        <label class="switch save">
          <input v-model="wizard.draft.saveRun" type="checkbox" />
          <span>{{ t("review.saveRun") }}</span>
        </label>
        <input
          v-model="wizard.draft.saveName"
          class="input"
          :disabled="!wizard.draft.saveRun"
          :placeholder="t('review.savedName')"
        />
        <span class="hint">{{ t("review.savedName") }}</span>
      </div>
    </section>
  </div>
</template>

<style scoped>
.chips {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.chip {
  cursor: default;
}

.hint {
  margin-top: 10px;
  font-size: 12px;
}

.facts {
  display: grid;
  grid-template-columns: 120px 1fr;
  gap: 8px 16px;
  margin: 0;
}

.facts dt {
  color: var(--text-muted);
  font-size: 12.5px;
  font-weight: 600;
}

.facts dd {
  margin: 0;
}

.save-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 6px 14px;
  align-items: end;
}

.save-grid .field {
  display: contents;
}

.save-grid .field > label {
  grid-row: 1;
  display: flex;
  align-items: center;
  min-height: 20px;
}

.save-grid .field > .input {
  grid-row: 2;
}

.save-grid .field > .hint {
  grid-row: 3;
}

.save {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--text-muted);
}
</style>
