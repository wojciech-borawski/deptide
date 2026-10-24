<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";

import type { CopyScope } from "@/api/types";
import AppIcon from "@/components/ui/AppIcon.vue";
import EmptyState from "@/components/ui/EmptyState.vue";
import NoticeBanner from "@/components/ui/NoticeBanner.vue";
import { routeNames } from "@/router";
import { useDependenciesStore } from "@/stores/dependencies";
import { useTerminalStore } from "@/stores/terminal";

const DEDUPE_COMMAND = "npm dedupe";

const dependencies = useDependenciesStore();
const terminal = useTerminalStore();
const router = useRouter();
const { t } = useI18n();

const scopes: CopyScope[] = ["libraries", "all-packages"];

const copies = computed(() => dependencies.copies);
const affected = computed(() => (copies.value ?? []).filter((project) => project.packages.length > 0));
const clean = computed(() =>
  (copies.value ?? []).filter((project) => project.hasNodeModules && project.packages.length === 0),
);
const withoutNodeModules = computed(() => (copies.value ?? []).filter((project) => !project.hasNodeModules));

function openDedupe(): void {
  terminal.prepareCommand(
    affected.value.map((project) => project.project),
    DEDUPE_COMMAND,
  );
  void router.push({ name: routeNames.terminal });
}
</script>

<template>
  <div class="stack">
    <p class="muted intro">{{ t("dependencies.duplicates.intro") }}</p>

    <div class="toolbar">
      <div class="scopes" role="radiogroup" :aria-label="t('dependencies.duplicates.scopeLabel')">
        <label v-for="scope in scopes" :key="scope" class="check">
          <input
            v-model="dependencies.copyScope"
            type="radio"
            name="copy-scope"
            :value="scope"
            :disabled="dependencies.copiesBusy"
          />
          {{ t(`dependencies.duplicates.scope.${scope}`) }}
        </label>
      </div>
      <span class="spacer" />
      <button class="btn btn-sm" type="button" :disabled="dependencies.copiesBusy" @click="dependencies.scanCopies">
        <span v-if="dependencies.copiesBusy" class="spinner" aria-hidden="true" />
        <AppIcon v-else name="search" :size="14" />
        {{ dependencies.copiesBusy ? t("dependencies.duplicates.scanning") : t("dependencies.duplicates.scan") }}
      </button>
      <button
        class="btn btn-primary btn-sm"
        type="button"
        :disabled="!affected.length || dependencies.copiesBusy || terminal.isActive"
        :title="terminal.isActive ? t('dependencies.duplicates.terminalBusy') : t('dependencies.duplicates.dedupeHint')"
        @click="openDedupe"
      >
        <AppIcon name="terminal" :size="14" />
        {{ t("dependencies.duplicates.runDedupe") }}
      </button>
    </div>

    <NoticeBanner v-if="dependencies.copiesError" tone="error" selectable>{{ dependencies.copiesError }}</NoticeBanner>

    <p v-if="dependencies.copiesBusy" class="muted" role="status">
      <span class="spinner" aria-hidden="true" /> {{ t("dependencies.duplicates.scanningHint") }}
    </p>

    <EmptyState
      v-else-if="!copies"
      icon="layers"
      :title="t('dependencies.duplicates.notScannedTitle')"
      :text="t('dependencies.duplicates.notScannedText')"
    />

    <template v-else>
      <NoticeBanner v-if="!affected.length" tone="ok">{{ t("dependencies.duplicates.none") }}</NoticeBanner>

      <section v-for="project in affected" :key="project.project" class="card project">
        <div class="card-title">
          <h3>{{ project.project }}</h3>
          <span class="muted">{{
            t("dependencies.duplicates.packages", { count: project.packages.length }, project.packages.length)
          }}</span>
        </div>
        <div v-for="entry in project.packages" :key="entry.name" class="package">
          <span class="mono name">{{ entry.name }}</span>
          <table class="copies">
            <caption class="sr-only">
              {{
                t("dependencies.duplicates.copiesOf", { name: entry.name, project: project.project })
              }}
            </caption>
            <thead class="sr-only">
              <tr>
                <th scope="col">{{ t("dependencies.duplicates.version") }}</th>
                <th scope="col">{{ t("dependencies.duplicates.path") }}</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="copy in entry.copies" :key="copy.path">
                <td class="mono version">{{ copy.version ?? "?" }}</td>
                <td class="mono path selectable">{{ copy.path }}</td>
              </tr>
            </tbody>
          </table>
        </div>
      </section>

      <p v-if="clean.length" class="muted">
        {{ t("dependencies.duplicates.clean", { names: clean.map((project) => project.project).join(", ") }) }}
      </p>
      <p v-if="withoutNodeModules.length" class="muted">
        {{
          t("dependencies.duplicates.noNodeModules", {
            names: withoutNodeModules.map((project) => project.project).join(", "),
          })
        }}
      </p>
    </template>
  </div>
</template>

<style scoped>
.intro {
  max-width: 760px;
}

.scopes {
  display: flex;
  flex-wrap: wrap;
  gap: 6px 14px;
}

.spacer {
  flex: 1;
}

.project {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.package {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.name {
  font-weight: 600;
}

.copies {
  border-collapse: collapse;
  font-size: 12.5px;
}

.copies td {
  padding: 2px 12px 2px 0;
  vertical-align: top;
}

.version {
  width: 7em;
  color: var(--accent);
  white-space: nowrap;
}

.path {
  color: var(--text-muted);
  word-break: break-all;
}
</style>
