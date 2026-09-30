<script setup lang="ts">
import { useI18n } from "vue-i18n";

import ModalDialog from "@/components/ui/ModalDialog.vue";

const props = defineProps<{ open: boolean; count: number }>();
const emit = defineEmits<{ cancel: []; confirm: [] }>();
const { t } = useI18n();
</script>

<template>
  <ModalDialog :open="props.open" :title="t('transfer.discardTitle')" width="480px" @close="emit('cancel')">
    <p class="text">{{ t("transfer.discardText", { count: props.count }, props.count) }}</p>
    <template #footer>
      <button class="btn" type="button" @click="emit('cancel')">{{ t("common.cancel") }}</button>
      <button class="btn btn-danger" type="button" @click="emit('confirm')">{{ t("transfer.discardConfirm") }}</button>
    </template>
  </ModalDialog>
</template>

<style scoped>
.text {
  margin: 0;
}
</style>
