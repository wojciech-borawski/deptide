<script setup lang="ts">
import { computed, ref, shallowRef, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";

import type { ProjectRestore, RunBackup } from "@/api/types";
import ActionButton from "@/components/ui/ActionButton.vue";
import AppIcon from "@/components/ui/AppIcon.vue";
import ModalDialog from "@/components/ui/ModalDialog.vue";
import NoticeBanner from "@/components/ui/NoticeBanner.vue";
import { useAsyncAction } from "@/composables/useAsyncAction";
import { routeNames } from "@/router";
import { useRestoreStore } from "@/stores/restore";
import { useTerminalStore } from "@/stores/terminal";
import { useWorkspaceStore } from "@/stores/workspace";

const NPM_INSTALL_COMMAND = "npm install";

const props = defineProps<{ open: boolean; backup: RunBackup | null; label: string }>();
const emit = defineEmits<{ close: [] }>();
const workspace = useWorkspaceStore();
const restoreStore = useRestoreStore();
const terminal = useTerminalStore();
const router = useRouter();
const { t } = useI18n();

const action = shallowRef(useAsyncAction());
const results = ref<ProjectRestore[] | null>(null);

const restoredNames = computed(() =>
  (results.value ?? []).filter((result) => result.outcome === "restored").map((result) => result.name),
);

watch(
  () => props.open,
  () => {
    action.value = useAsyncAction();
    results.value = null;
  },
);

async function confirm(): Promise<void> {
  const backup = props.backup;
  if (!backup) return;
  const current = action.value;
  const outcome = await current.run(() => restoreStore.restore(workspace.root, backup));
  if (current !== action.value) return;
  if (outcome) results.value = outcome;
}

function resultText(result: ProjectRestore): string {
  return result.outcome === "restored"
    ? t("restore.restored", { name: result.name, files: result.files.join(", ") })
    : t("restore.failed", { name: result.name, error: result.error });
}

function openNpmInstall(): void {
  terminal.prepareCommand(restoredNames.value, NPM_INSTALL_COMMAND);
  emit("close");
  void router.push({ name: routeNames.terminal });
}
</script>

<template>
  <ModalDialog
    :open="props.open"
    :title="t('restore.title', { label: props.label })"
    width="560px"
    @close="emit('close')"
  >
    <div v-if="props.backup" class="stack">
      <template v-if="!results">
        <p class="muted">{{ t("restore.intro") }}</p>
        <ul class="projects mono">
          <li v-for="project in props.backup.projects" :key="project.name">
            {{ t("restore.files", { name: project.name, files: project.files.join(", ") }) }}
          </li>
        </ul>
      </template>
      <template v-else>
        <ul class="projects mono">
          <li v-for="result in results" :key="result.name" :class="result.outcome">
            <AppIcon :name="result.outcome === 'restored' ? 'check' : 'warning'" :size="13" />
            {{ resultText(result) }}
          </li>
        </ul>
        <NoticeBanner v-if="restoredNames.length" tone="info">{{ t("restore.npmHint") }}</NoticeBanner>
      </template>
      <NoticeBanner v-if="action.error.value" tone="error" selectable>{{ action.error.value }}</NoticeBanner>
    </div>
    <template #footer>
      <button class="btn" type="button" @click="emit('close')">
        {{ results ? t("common.close") : t("common.cancel") }}
      </button>
      <ActionButton
        v-if="!results && props.backup"
        variant="danger"
        icon="history"
        :busy="action.busy.value"
        @click="confirm"
      >
        {{ t("restore.confirm", props.backup.projects.length) }}
      </ActionButton>
      <button v-if="restoredNames.length" class="btn btn-primary" type="button" @click="openNpmInstall">
        <AppIcon name="terminal" :size="15" />
        {{ t("restore.openTerminal") }}
      </button>
    </template>
  </ModalDialog>
</template>

<style scoped>
.projects {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin: 0;
  padding-left: 0;
  list-style: none;
  font-size: 12.5px;
}

.projects li {
  display: flex;
  align-items: center;
  gap: 6px;
}

.restored {
  color: var(--ok);
}

.failed {
  color: var(--failed);
}
</style>
