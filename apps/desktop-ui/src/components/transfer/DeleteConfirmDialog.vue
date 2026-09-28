<script setup lang="ts">
import { useI18n } from "vue-i18n";

import ModalDialog from "@/components/ui/ModalDialog.vue";
import type { RemovalGroup } from "@/lib/receive-view";

const props = defineProps<{ open: boolean; groups: RemovalGroup[] }>();
const emit = defineEmits<{ cancel: []; confirm: [] }>();
const { t } = useI18n();
</script>

<template>
  <ModalDialog :open="props.open" :title="t('transfer.deleteTitle')" width="560px" @close="emit('cancel')">
    <p class="intro">{{ t("transfer.deleteText") }}</p>
    <section v-for="group in props.groups" :key="group.source" class="group">
      <h4>
        {{ group.target }} <span class="muted count">{{ group.files.length }}</span>
      </h4>
      <ul class="mono selectable">
        <li v-for="path in group.files" :key="path">{{ path }}</li>
      </ul>
    </section>
    <template #footer>
      <button class="btn" type="button" @click="emit('cancel')">{{ t("common.cancel") }}</button>
      <button class="btn btn-danger" type="button" @click="emit('confirm')">{{ t("transfer.deleteConfirm") }}</button>
    </template>
  </ModalDialog>
</template>

<style scoped>
.intro {
  margin: 0 0 12px;
}

.group + .group {
  margin-top: 12px;
}

h4 {
  margin: 0 0 6px;
  font-size: 13px;
}

.count {
  font-weight: 400;
}

ul {
  margin: 0;
  padding-left: 18px;
  display: grid;
  gap: 2px;
  color: var(--failed);
}
</style>
