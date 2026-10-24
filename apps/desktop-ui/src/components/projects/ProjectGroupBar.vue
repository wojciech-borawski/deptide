<script setup lang="ts">
import { computed, nextTick, ref } from "vue";
import { useI18n } from "vue-i18n";

import type { ProjectGroup, ProjectView } from "@/api/types";
import AppIcon from "@/components/ui/AppIcon.vue";
import ModalDialog from "@/components/ui/ModalDialog.vue";
import NoticeBanner from "@/components/ui/NoticeBanner.vue";
import { describeError } from "@/composables/useAsyncAction";
import { addGroupToSelection, groupSelection, renameGroup, withGroup, withoutGroup } from "@/lib/project-groups";
import { useWorkspaceStore } from "@/stores/workspace";

type Dialog = { kind: "save" } | { kind: "rename"; group: ProjectGroup } | { kind: "delete"; group: ProjectGroup };

const props = defineProps<{ available: readonly ProjectView[]; selected: readonly string[]; disabled?: boolean }>();
const emit = defineEmits<{ select: [names: string[]] }>();
const workspace = useWorkspaceStore();
const { t } = useI18n();

const groups = computed(() => workspace.config?.groups ?? []);
const editing = ref(false);
const dialog = ref<Dialog | null>(null);
const name = ref("");
const conflict = ref(false);
const nameInput = ref<HTMLInputElement | null>(null);
const writePending = ref(false);
const writeError = ref("");
let dialogsOpened = 0;

const trimmedName = computed(() => name.value.trim());
const dialogTitle = computed(() => {
  if (dialog.value?.kind === "rename") return t("projects.groups.renameTitle");
  if (dialog.value?.kind === "delete") return t("projects.groups.deleteTitle");
  return t("projects.groups.saveSelection");
});

function isNameTaken(candidate: string, except?: ProjectGroup): boolean {
  return groups.value.some((group) => group.name === candidate && group.name !== except?.name);
}

function pick(group: ProjectGroup, event: MouseEvent): void {
  const adding = event.shiftKey || event.ctrlKey || event.metaKey;
  emit(
    "select",
    adding ? addGroupToSelection(group, props.available, props.selected) : groupSelection(group, props.available),
  );
}

async function saveGroups(next: ProjectGroup[]): Promise<boolean> {
  const config = workspace.config;
  if (!config || writePending.value) return false;
  const opened = dialogsOpened;
  writePending.value = true;
  writeError.value = "";
  try {
    await workspace.updateConfig({ ...config, groups: next });
    return opened === dialogsOpened;
  } catch (cause) {
    if (opened === dialogsOpened) writeError.value = describeError(cause);
    return false;
  } finally {
    writePending.value = false;
  }
}

function open(next: Dialog): void {
  name.value = next.kind === "rename" ? next.group.name : "";
  conflict.value = false;
  writeError.value = "";
  dialogsOpened += 1;
  dialog.value = next;
  void nextTick(() => nameInput.value?.focus());
}

function close(): void {
  dialog.value = null;
}

function onNameInput(event: Event): void {
  name.value = (event.target as HTMLInputElement).value;
  conflict.value = false;
}

async function save(): Promise<void> {
  if (!trimmedName.value) return;
  if (!conflict.value && isNameTaken(trimmedName.value)) {
    conflict.value = true;
    return;
  }
  const saved = await saveGroups(withGroup(groups.value, trimmedName.value, props.selected));
  if (saved) close();
}

async function rename(group: ProjectGroup): Promise<void> {
  if (!trimmedName.value) return;
  if (isNameTaken(trimmedName.value, group)) {
    conflict.value = true;
    return;
  }
  const saved = await saveGroups(renameGroup(groups.value, group.name, trimmedName.value));
  if (saved) close();
}

async function remove(group: ProjectGroup): Promise<void> {
  const saved = await saveGroups(withoutGroup(groups.value, group.name));
  if (saved) close();
}

function submit(): void {
  const current = dialog.value;
  if (current?.kind === "save") void save();
  else if (current?.kind === "rename") void rename(current.group);
}
</script>

<template>
  <div class="groups" role="group" :aria-label="t('projects.groups.label')">
    <span class="label muted" :title="t('projects.groups.hint')">{{ t("projects.groups.label") }}</span>
    <span v-for="group in groups" :key="group.name" class="group" :class="{ editing }">
      <button
        class="chip"
        type="button"
        :disabled="props.disabled"
        :title="`${t('projects.groups.members', { names: group.projects.join(', ') })}\n${t('projects.groups.hint')}`"
        @click="pick(group, $event)"
      >
        {{ group.name }}
      </button>
      <template v-if="editing">
        <button
          class="btn btn-ghost btn-icon"
          type="button"
          :aria-label="t('projects.groups.rename', { name: group.name })"
          :title="t('projects.groups.rename', { name: group.name })"
          @click="open({ kind: 'rename', group })"
        >
          <AppIcon name="edit" :size="13" />
        </button>
        <button
          class="btn btn-ghost btn-icon"
          type="button"
          :aria-label="t('projects.groups.delete', { name: group.name })"
          :title="t('projects.groups.delete', { name: group.name })"
          @click="open({ kind: 'delete', group })"
        >
          <AppIcon name="trash" :size="13" />
        </button>
      </template>
    </span>
    <span v-if="!groups.length" class="muted small">{{ t("projects.groups.none") }}</span>
    <span class="spacer" />
    <button
      class="btn btn-ghost btn-sm"
      type="button"
      :disabled="!props.selected.length"
      @click="open({ kind: 'save' })"
    >
      <AppIcon name="plus" :size="13" />
      {{ t("projects.groups.saveSelection") }}
    </button>
    <button
      v-if="groups.length"
      class="btn btn-ghost btn-sm"
      type="button"
      :aria-pressed="editing"
      @click="editing = !editing"
    >
      {{ editing ? t("projects.groups.doneEditing") : t("projects.groups.edit") }}
    </button>

    <ModalDialog :open="dialog !== null" :title="dialogTitle" width="440px" @close="close">
      <p v-if="dialog?.kind === 'delete'">{{ t("projects.groups.deleteText", { name: dialog.group.name }) }}</p>
      <form v-else class="stack" @submit.prevent="submit">
        <div class="field">
          <label for="project-group-name">{{ t("projects.groups.name") }}</label>
          <input
            id="project-group-name"
            ref="nameInput"
            class="input"
            :value="name"
            maxlength="80"
            @input="onNameInput"
          />
          <span v-if="dialog?.kind === 'save'" class="hint">{{
            t("projects.groups.members", { names: props.selected.join(", ") })
          }}</span>
        </div>
        <p v-if="conflict && dialog?.kind === 'save'" class="warning-text">
          {{ t("projects.groups.exists", { name: trimmedName }) }}
        </p>
        <p v-if="conflict && dialog?.kind === 'rename'" class="warning-text">
          {{ t("projects.groups.renameTaken", { name: trimmedName }) }}
        </p>
      </form>
      <NoticeBanner v-if="writeError" tone="error" selectable>{{ writeError }}</NoticeBanner>
      <template #footer>
        <button class="btn" type="button" @click="close">{{ t("common.cancel") }}</button>
        <button
          v-if="dialog?.kind === 'delete'"
          class="btn btn-danger"
          type="button"
          :disabled="writePending"
          @click="remove(dialog.group)"
        >
          {{ t("common.delete") }}
        </button>
        <button
          v-else-if="dialog?.kind === 'rename'"
          class="btn btn-primary"
          type="button"
          :disabled="writePending"
          @click="submit"
        >
          {{ t("projects.groups.renameConfirm") }}
        </button>
        <button v-else-if="conflict" class="btn btn-danger" type="button" :disabled="writePending" @click="submit">
          {{ t("projects.groups.replace") }}
        </button>
        <button v-else class="btn btn-primary" type="button" :disabled="!trimmedName || writePending" @click="submit">
          {{ t("projects.groups.save") }}
        </button>
      </template>
    </ModalDialog>
  </div>
</template>

<style scoped>
.groups {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
}

.group {
  display: inline-flex;
  align-items: center;
  gap: 2px;
}

.group.editing {
  padding-right: 4px;
  border-right: 1px solid var(--border);
}

.label,
.small {
  font-size: 12px;
}

.warning-text {
  color: var(--warn);
}
</style>
