<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";

import VersionPicker from "./VersionPicker.vue";
import AppIcon from "@/components/ui/AppIcon.vue";
import NoticeBanner from "@/components/ui/NoticeBanner.vue";
import { useCountedNoun } from "@/composables/useCountedNoun";
import { formatSpec } from "@/lib/versions";
import type { PackageChoice } from "@/lib/wizard-plan";
import { useWizardStore } from "@/stores/wizard";
import { useWorkspaceStore } from "@/stores/workspace";

const workspace = useWorkspaceStore();
const wizard = useWizardStore();
const { t } = useI18n();
const countOf = useCountedNoun();

const filter = ref("");
const localOnly = ref(false);
const manualText = ref("");
const manualError = ref("");

const visible = computed(() => {
  const needle = filter.value.trim().toLowerCase();

  return wizard.draft.choices.filter((choice) => {
    if (localOnly.value && !choice.localVersion && !choice.selected) return false;
    return !needle || choice.name.toLowerCase().includes(needle);
  });
});

const selectedCount = computed(() => wizard.draft.choices.filter((choice) => choice.selected).length);
const localCount = computed(() => wizard.draft.choices.filter((choice) => choice.localVersion).length);

function patch(choice: PackageChoice, changes: Partial<PackageChoice>): void {
  Object.assign(choice, changes);
}

function addManual(): void {
  const problem = wizard.addManualPackage(manualText.value);
  manualError.value = problem ? t(problem) : "";
  if (!problem) manualText.value = "";
}

function usage(choice: PackageChoice): string {
  const parts = [t("libraries.usedBy", { count: countOf("project", choice.usedBy.length) })];
  const names = choice.usedBy.slice(0, 4).join(", ") + (choice.usedBy.length > 4 ? ", …" : "");
  parts.push(names);
  if (choice.currentRanges.length) parts.push(t("libraries.currently", { ranges: choice.currentRanges.join(", ") }));
  return parts.join(" · ");
}

onMounted(() => {
  void wizard.loadCandidates(workspace.root);
});
</script>

<template>
  <div class="stack">
    <div class="toolbar">
      <div class="search">
        <AppIcon name="search" :size="15" />
        <input v-model="filter" class="input input-sm" :placeholder="t('libraries.filter')" />
      </div>
      <label class="switch">
        <input v-model="localOnly" type="checkbox" />
        <span>{{ t("libraries.onlyLocal", { count: localCount }) }}</span>
      </label>
      <span class="spacer" />
      <span class="muted">{{ t("common.selected", { count: selectedCount }) }}</span>
      <button
        class="btn btn-ghost btn-sm"
        type="button"
        :disabled="wizard.inspecting"
        @click="wizard.loadCandidates(workspace.root)"
      >
        <AppIcon name="refresh" :size="14" />
        {{ t("common.rescan") }}
      </button>
    </div>

    <div v-if="wizard.inspecting" class="row muted">
      <span class="spinner" />
      {{
        t("libraries.reading", {
          count: countOf("project", wizard.draft.projectNames.length),
        })
      }}
    </div>
    <NoticeBanner v-if="wizard.inspectionError" tone="error" selectable>{{ wizard.inspectionError }}</NoticeBanner>
    <NoticeBanner v-if="wizard.inspection?.unreadable.length" tone="warn">
      {{ t("libraries.unreadable", { names: wizard.inspection.unreadable.join(", ") }) }}
    </NoticeBanner>

    <div v-if="!wizard.inspecting && wizard.draft.choices.length" class="list">
      <div v-for="choice in visible" :key="choice.name" class="entry" :class="{ selected: choice.selected }">
        <label class="list-row candidate">
          <span class="check">
            <input type="checkbox" :checked="choice.selected" @change="patch(choice, { selected: !choice.selected })" />
          </span>
          <span class="details">
            <span class="row">
              <span class="name mono">{{ choice.name }}</span>
              <span v-if="choice.localVersion" class="badge badge-accent mono">{{
                t("libraries.local", { version: choice.localVersion })
              }}</span>
              <span v-if="choice.localBranch" class="badge badge-violet mono" :title="choice.localBranch">
                {{ t("libraries.branch", { branch: choice.branchSuffix ?? choice.localBranch }) }}
              </span>
              <span v-if="choice.saveDev" class="badge">{{ t("libraries.dev") }}</span>
            </span>
            <span class="muted usage" :title="choice.usedBy.join(', ')">{{ usage(choice) }}</span>
          </span>
        </label>
        <VersionPicker v-if="choice.selected" :choice="choice" @patch="patch(choice, $event)" />
      </div>
      <p v-if="!visible.length" class="list-row muted">{{ t("libraries.noMatch") }}</p>
    </div>

    <NoticeBanner v-else-if="!wizard.inspecting && !wizard.inspectionError" tone="info">
      {{ t("libraries.noneFound") }}
    </NoticeBanner>

    <section class="card">
      <div class="card-title">
        <h3>{{ t("libraries.additional") }}</h3>
        <span class="muted">{{ t("libraries.additionalHint") }}</span>
      </div>
      <div class="row">
        <input
          v-model="manualText"
          class="input mono"
          :placeholder="t('libraries.manualPlaceholder')"
          @keydown.enter.stop.prevent="addManual"
        />
        <button class="btn" type="button" @click="addManual">
          <AppIcon name="plus" :size="15" />
          {{ t("common.add") }}
        </button>
      </div>
      <NoticeBanner v-if="manualError" tone="warn" class="manual-error">{{ manualError }}</NoticeBanner>
      <div v-if="wizard.draft.manualPackages.length" class="chips">
        <span v-for="spec in wizard.draft.manualPackages" :key="spec.name" class="chip active mono">
          {{ formatSpec(spec) }}
          <button
            class="chip-remove"
            type="button"
            :aria-label="t('libraries.removePackage', { name: spec.name })"
            @click="wizard.removeManualPackage(spec.name)"
          >
            <AppIcon name="close" :size="12" />
          </button>
        </span>
      </div>
    </section>
  </div>
</template>

<style scoped>
.search {
  display: flex;
  align-items: center;
  gap: 8px;
  padding-left: 10px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--bg-input);
  color: var(--text-muted);
  width: 280px;
}

.search .input {
  border: none;
  background: transparent;
  box-shadow: none;
}

.entry {
  border-bottom: 1px solid var(--border);
}

.entry:last-child {
  border-bottom: none;
}

.entry.selected {
  background: var(--accent-soft);
}

.entry .list-row {
  border-bottom: none;
}

.candidate {
  cursor: pointer;
}

.details {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
  flex: 1;
}

.name {
  font-weight: 600;
  font-size: 13px;
}

.usage {
  font-size: 12px;
}

.chips {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-top: 12px;
}

.chip-remove {
  display: grid;
  place-items: center;
  border: none;
  background: transparent;
  color: inherit;
  padding: 0;
  cursor: pointer;
}

.manual-error {
  margin-top: 10px;
}
</style>
