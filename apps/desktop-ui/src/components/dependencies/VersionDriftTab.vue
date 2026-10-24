<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";

import type { DriftCell, LibraryDrift } from "@/api/types";
import AppIcon from "@/components/ui/AppIcon.vue";
import EmptyState from "@/components/ui/EmptyState.vue";
import NoticeBanner from "@/components/ui/NoticeBanner.vue";
import SearchBox from "@/components/ui/SearchBox.vue";
import {
  filterDrift,
  isMismatch,
  selectRow,
  toggleCell,
  versionPicks,
  type DriftFilter,
  type DriftSelection,
} from "@/lib/dependency-drift";
import { sectionField } from "@/lib/dependency-sections";
import { routeNames } from "@/router";
import { useDependenciesStore } from "@/stores/dependencies";
import { useWizardStore } from "@/stores/wizard";
import { useWorkspaceStore } from "@/stores/workspace";

const dependencies = useDependenciesStore();
const workspace = useWorkspaceStore();
const wizard = useWizardStore();
const router = useRouter();
const { t } = useI18n();

const filter = reactive<DriftFilter>({ library: "", project: "", onlyMismatches: false });
const selection = ref<DriftSelection | null>(null);
const pickedVersion = ref<string | null>(null);

const view = computed(() =>
  dependencies.drift ? filterDrift(dependencies.drift, filter) : { projects: [], libraries: [] },
);
const rows = computed(() =>
  view.value.libraries.map((library) => ({
    library,
    cells: view.value.projects.map((project) => ({ project, cell: cellOf(library, project) })),
  })),
);
const selectedLibrary = computed(
  () => dependencies.drift?.libraries.find((library) => library.name === selection.value?.library) ?? null,
);
const picks = computed(() => (selectedLibrary.value ? versionPicks(selectedLibrary.value) : []));

watch(
  () => selection.value?.library,
  () => {
    pickedVersion.value = null;
  },
);

function cellOf(library: LibraryDrift, project: string): DriftCell | null {
  return library.cells.find((cell) => cell.project === project) ?? null;
}

function isSelected(library: LibraryDrift, project: string): boolean {
  return selection.value?.library === library.name && selection.value.projects.includes(project);
}

function toggle(library: LibraryDrift, project: string): void {
  selection.value = toggleCell(selection.value, library.name, project);
}

function pickRow(library: LibraryDrift): void {
  selection.value = selectRow(library, view.value.projects);
}

function cellTitle(cell: DriftCell, library: LibraryDrift): string {
  const lines = [
    `${sectionField[cell.section]}: ${cell.declared}`,
    cell.installed
      ? t("dependencies.versions.installed", { version: cell.installed })
      : t("dependencies.versions.notInstalled"),
  ];
  if (cell.outOfRange) lines.push(t("dependencies.versions.outOfRangeHint"));
  if (cell.differs) lines.push(t("dependencies.versions.differsHint"));
  if (cell.behind && library.latest) lines.push(t("dependencies.versions.behindHint", { latest: library.latest }));
  return lines.join("\n");
}

function updateInWizard(): void {
  const chosen = selection.value;
  const settings = workspace.settings;
  if (!chosen || !settings) return;
  wizard.ensureInitialized(workspace.root, settings, workspace.projects);
  wizard.prefillLibrary(
    { projectNames: chosen.projects, library: chosen.library, version: pickedVersion.value },
    settings,
  );
  void router.push({ name: routeNames.wizard });
}
</script>

<template>
  <div class="stack">
    <p class="muted intro">{{ t("dependencies.versions.intro") }}</p>

    <div class="toolbar">
      <SearchBox v-model="filter.library" :placeholder="t('dependencies.versions.filterLibrary')" width="220px" />
      <SearchBox v-model="filter.project" :placeholder="t('dependencies.versions.filterProject')" width="200px" />
      <label class="check">
        <input v-model="filter.onlyMismatches" type="checkbox" />
        {{ t("dependencies.versions.onlyMismatches") }}
      </label>
      <span class="spacer" />
      <button
        class="btn btn-ghost btn-sm"
        type="button"
        :disabled="dependencies.driftBusy"
        @click="dependencies.loadDrift(false)"
      >
        <AppIcon name="refresh" :size="14" />
        {{ t("common.refresh") }}
      </button>
      <button
        class="btn btn-sm"
        type="button"
        :disabled="dependencies.driftBusy"
        :title="t('dependencies.versions.checkLatestHint')"
        @click="dependencies.loadDrift(true)"
      >
        <span v-if="dependencies.checkingLatest" class="spinner" aria-hidden="true" />
        <AppIcon v-else name="external" :size="14" />
        {{
          dependencies.checkingLatest
            ? t("dependencies.versions.checkingLatest")
            : t("dependencies.versions.checkLatest")
        }}
      </button>
    </div>

    <NoticeBanner v-if="dependencies.driftError" tone="error" selectable>{{ dependencies.driftError }}</NoticeBanner>
    <NoticeBanner v-if="dependencies.drift?.unreadable.length" tone="warn">
      {{ t("dependencies.unreadable", { names: dependencies.drift.unreadable.join(", ") }) }}
    </NoticeBanner>

    <p v-if="dependencies.driftBusy && !dependencies.drift" class="muted" role="status">
      <span class="spinner" aria-hidden="true" /> {{ t("dependencies.versions.loading") }}
    </p>
    <EmptyState
      v-else-if="dependencies.drift && !dependencies.drift.libraries.length"
      icon="package"
      :title="t('dependencies.versions.emptyTitle')"
      :text="t('dependencies.versions.emptyText')"
    />
    <p v-else-if="dependencies.drift && !view.libraries.length" class="muted">
      {{ t("dependencies.versions.noMatches") }}
    </p>

    <div v-else-if="dependencies.drift" class="table-wrap">
      <table class="drift" :aria-busy="dependencies.driftBusy">
        <caption class="sr-only">
          {{
            t("dependencies.versions.caption")
          }}
        </caption>
        <thead>
          <tr>
            <th scope="col" class="library-head">{{ t("dependencies.versions.library") }}</th>
            <th v-for="project in view.projects" :key="project" scope="col" class="project-head">
              <span class="truncate" :title="project">{{ project }}</span>
            </th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="{ library, cells } in rows" :key="library.name" :class="{ mismatch: library.mismatch }">
            <th scope="row" class="library">
              <button
                class="row-button"
                type="button"
                :aria-pressed="selection?.library === library.name"
                :title="t('dependencies.versions.selectRow', { library: library.name })"
                @click="pickRow(library)"
              >
                <span class="mono name">{{ library.name }}</span>
              </button>
              <span class="facts">
                <span v-if="library.localVersion" class="badge badge-violet">
                  {{ t("dependencies.versions.local", { version: library.localVersion }) }}
                </span>
                <span v-else-if="library.ambiguousProjects.length" class="badge badge-warn">
                  {{ t("dependencies.versions.localAmbiguous", { projects: library.ambiguousProjects.join(", ") }) }}
                </span>
                <span v-if="library.latest" class="badge badge-accent">
                  {{ t("dependencies.versions.latest", { version: library.latest }) }}
                </span>
                <span v-if="library.latestError" class="latest-error" :title="library.latestError">
                  <AppIcon name="warning" :size="12" />
                  {{ t("dependencies.versions.latestFailed") }}
                </span>
              </span>
            </th>
            <td v-for="{ project, cell } in cells" :key="project">
              <button
                v-if="cell"
                class="cell"
                :class="{ selected: isSelected(library, project), problem: isMismatch(cell) }"
                type="button"
                :aria-pressed="isSelected(library, project)"
                :aria-label="`${library.name}, ${project}: ${cellTitle(cell, library)}`"
                :title="cellTitle(cell, library)"
                @click="toggle(library, project)"
              >
                <span class="mono installed">{{ cell.installed ?? t("dependencies.versions.none") }}</span>
                <span class="mono declared">
                  {{ cell.declared
                  }}<template v-if="cell.section !== 'dependencies'">
                    · {{ t(`libraries.sectionBadge.${cell.section}`) }}</template
                  >
                </span>
                <span class="markers">
                  <span v-if="cell.outOfRange" class="badge badge-failed">{{
                    t("dependencies.versions.outOfRange")
                  }}</span>
                  <span v-if="cell.differs" class="badge badge-warn">{{ t("dependencies.versions.differs") }}</span>
                  <span v-if="cell.behind" class="badge badge-running">{{ t("dependencies.versions.behind") }}</span>
                </span>
              </button>
              <span v-else class="faint" :aria-label="t('dependencies.versions.notDeclared')">–</span>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <div v-if="selection" class="selection card" role="region" :aria-label="t('dependencies.versions.selectionLabel')">
      <span class="summary">
        {{
          t("dependencies.versions.selection", { library: selection.library, projects: selection.projects.join(", ") })
        }}
      </span>
      <div class="picks" role="radiogroup" :aria-label="t('dependencies.versions.pickVersion')">
        <label class="check">
          <input v-model="pickedVersion" type="radio" :value="null" name="drift-version" />
          {{ t("dependencies.versions.pick.proposed") }}
        </label>
        <label v-for="pick in picks" :key="pick.kind" class="check">
          <input v-model="pickedVersion" type="radio" :value="pick.version" name="drift-version" />
          {{ t(`dependencies.versions.pick.${pick.kind}`, { version: pick.version }) }}
        </label>
      </div>
      <span class="spacer" />
      <button class="btn btn-ghost btn-sm" type="button" @click="selection = null">
        {{ t("dependencies.versions.clearSelection") }}
      </button>
      <button class="btn btn-primary btn-sm" type="button" @click="updateInWizard">
        <AppIcon name="update" :size="14" />
        {{ t("dependencies.versions.updateInWizard") }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.intro {
  max-width: 760px;
}

.table-wrap {
  overflow: auto;
  border: 1px solid var(--border);
  border-radius: var(--radius);
}

.drift {
  border-collapse: separate;
  border-spacing: 0;
  min-width: 100%;
  font-size: 12.5px;
}

.drift th,
.drift td {
  padding: 6px 8px;
  border-bottom: 1px solid var(--border);
  text-align: left;
  vertical-align: top;
}

.drift thead th {
  position: sticky;
  top: 0;
  z-index: 1;
  background: var(--bg-panel);
  color: var(--text-muted);
  font-weight: 600;
}

.project-head {
  max-width: 140px;
}

.project-head .truncate {
  display: block;
}

.library-head,
.library {
  position: sticky;
  left: 0;
  background: var(--bg-panel);
  min-width: 190px;
}

.library-head {
  z-index: 2;
}

tr.mismatch .library {
  box-shadow: inset 3px 0 0 var(--warn);
}

.row-button {
  display: block;
  padding: 0;
  border: none;
  background: transparent;
  color: var(--text);
  font: inherit;
  text-align: left;
  cursor: pointer;
}

.row-button[aria-pressed="true"] .name {
  color: var(--accent);
}

.name {
  font-weight: 600;
}

.facts {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  margin-top: 4px;
}

.latest-error {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  color: var(--failed);
  font-size: 11.5px;
}

.cell {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 2px;
  width: 100%;
  min-width: 96px;
  padding: 4px 6px;
  border: 1px solid transparent;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text);
  font: inherit;
  text-align: left;
  cursor: pointer;
}

.cell:hover {
  background: var(--bg-hover);
}

.cell.problem {
  background: var(--warn-soft);
}

.cell.selected {
  border-color: var(--accent);
  background: var(--accent-soft);
}

.row-button:focus-visible,
.cell:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}

.installed {
  font-weight: 600;
  white-space: nowrap;
}

.declared {
  color: var(--text-muted);
  font-size: 11.5px;
  white-space: nowrap;
}

.markers {
  display: flex;
  flex-wrap: wrap;
  gap: 3px;
}

.markers .badge {
  font-size: 10.5px;
}

.selection {
  position: sticky;
  bottom: 0;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 10px 16px;
  box-shadow: var(--shadow);
}

.summary {
  font-weight: 600;
}

.picks {
  display: flex;
  flex-wrap: wrap;
  gap: 6px 14px;
}

.spacer {
  flex: 1;
}
</style>
