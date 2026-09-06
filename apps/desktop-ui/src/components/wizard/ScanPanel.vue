<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";

import * as api from "@/api/commands";
import type { ScanResult, ScannedProject } from "@/api/types";
import ModalDialog from "@/components/ui/ModalDialog.vue";
import ProjectKindIcon from "@/components/ui/ProjectKindIcon.vue";
import ActionButton from "@/components/ui/ActionButton.vue";
import NoticeBanner from "@/components/ui/NoticeBanner.vue";
import { useAsyncAction } from "@/composables/useAsyncAction";
import { routeNames } from "@/router";
import { useWorkspaceStore } from "@/stores/workspace";

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{
  close: [];
  applied: [summary: { added: string[]; removed: string[] }];
}>();

const workspace = useWorkspaceStore();
const router = useRouter();
const { t } = useI18n();

const scanAction = useAsyncAction();
const applyAction = useAsyncAction();
const scanning = scanAction.busy;
const applying = applyAction.busy;
const error = computed(() => scanAction.error.value || applyAction.error.value);

const result = ref<ScanResult | null>(null);
const selected = ref<Set<string>>(new Set());
const filter = ref("");

const needsRoot = computed(() => !workspace.settings?.projectsRoot);

const visible = computed(() => {
  const needle = filter.value.trim().toLowerCase();
  const projects = result.value?.projects ?? [];
  if (!needle) return projects;

  return projects.filter(
    (project) =>
      project.proposedName.toLowerCase().includes(needle) ||
      project.relativePath.toLowerCase().includes(needle) ||
      (project.packageName ?? "").toLowerCase().includes(needle),
  );
});

const configuredCount = computed(() => result.value?.projects.filter((project) => project.configuredName).length ?? 0);

async function scan(): Promise<void> {
  if (needsRoot.value) return;

  result.value = null;

  await scanAction.run(async () => {
    const scanned = await api.scanProjects(workspace.root);
    result.value = scanned;
    selected.value = new Set(
      scanned.projects.filter((project) => project.configuredName).map((project) => project.directory),
    );
  });
}

function toggle(project: ScannedProject): void {
  const next = new Set(selected.value);
  if (next.has(project.directory)) next.delete(project.directory);
  else next.add(project.directory);
  selected.value = next;
}

function setAll(checked: boolean): void {
  selected.value = checked ? new Set(visible.value.map((project) => project.directory)) : new Set();
}

async function apply(): Promise<void> {
  if (!result.value) return;

  const projects = result.value.projects;

  await applyAction.run(async () => {
    const outcome = await api.applyScan(workspace.root, projects, [...selected.value]);
    await workspace.refresh();
    emit("applied", { added: outcome.added, removed: outcome.removed });
    emit("close");
  });
}

function goToSettings(): void {
  emit("close");
  void router.push({ name: routeNames.settings });
}

watch(
  () => props.open,
  (open) => {
    if (open) void scan();
  },
);
</script>

<template>
  <ModalDialog :open="props.open" :title="t('scan.title')" width="820px" @close="emit('close')">
    <div class="stack">
      <div v-if="needsRoot" class="notice notice-warn row">
        <span>{{ t("scan.needsRoot") }}</span>
        <span class="spacer" />
        <button class="btn btn-sm" type="button" @click="goToSettings">{{ t("scan.openSettings") }}</button>
      </div>

      <template v-else>
        <p class="muted">
          {{
            t("scan.description", {
              root: workspace.settings?.projectsRoot ?? "",
              depth: workspace.settings?.scanDepth ?? 0,
            })
          }}
        </p>

        <div v-if="scanning" class="row muted"><span class="spinner" /> {{ t("scan.looking") }}</div>
        <NoticeBanner v-if="error" tone="error" selectable>{{ error }}</NoticeBanner>

        <template v-if="result">
          <div class="toolbar">
            <input v-model="filter" class="input input-sm filter" :placeholder="t('scan.filter')" />
            <button class="btn btn-sm" type="button" @click="setAll(true)">{{ t("scan.tickAll") }}</button>
            <button class="btn btn-sm" type="button" @click="setAll(false)">{{ t("scan.untickAll") }}</button>
            <span class="spacer" />
            <span class="muted">{{
              t("scan.found", { found: result.projects.length, configured: configuredCount, ticked: selected.size })
            }}</span>
          </div>

          <div class="list scroll">
            <label
              v-for="project in visible"
              :key="project.directory"
              class="list-row candidate"
              :class="{ selected: selected.has(project.directory) }"
            >
              <span class="check">
                <input type="checkbox" :checked="selected.has(project.directory)" @change="toggle(project)" />
              </span>
              <ProjectKindIcon :kind="project.kind" />
              <span class="details">
                <span class="row">
                  <span class="name">{{ project.configuredName ?? project.proposedName }}</span>
                  <span v-if="project.packageName" class="badge badge-violet mono">{{ project.packageName }}</span>
                  <span v-if="project.version" class="badge mono">{{ project.version }}</span>
                  <span v-if="project.branch" class="badge mono">{{ project.branch }}</span>
                  <span v-if="project.configuredName" class="badge badge-accent">{{ t("scan.configured") }}</span>
                  <span v-if="project.hasBuildScript" class="badge">{{ t("scan.buildScript") }}</span>
                </span>
                <span class="mono muted truncate selectable" :title="project.directory">{{ project.directory }}</span>
              </span>
            </label>
            <p v-if="!visible.length" class="list-row muted">{{ t("scan.noMatch") }}</p>
          </div>
        </template>
      </template>
    </div>

    <template #footer>
      <button class="btn" type="button" @click="emit('close')">{{ t("common.cancel") }}</button>
      <ActionButton variant="primary" :busy="applying" :disabled="!result" @click="apply">
        {{ t("scan.apply") }}
      </ActionButton>
    </template>
  </ModalDialog>
</template>

<style scoped>
.filter {
  width: 240px;
}

.scroll {
  max-height: 46vh;
  overflow: auto;
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
}
</style>
