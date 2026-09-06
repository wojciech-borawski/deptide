<script setup lang="ts">
import AppIcon from "./AppIcon.vue";

const text = defineModel<string>({ required: true });
const props = withDefaults(defineProps<{ placeholder: string; width?: string }>(), { width: "auto" });
</script>

<template>
  <div class="search" :style="{ width: props.width }">
    <AppIcon name="search" :size="15" />
    <input v-model="text" class="input input-sm" :placeholder="props.placeholder" />
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
  min-width: 0;
}

.search .input {
  border: none;
  background: transparent;
  box-shadow: none;
  min-width: 0;
  flex: 1;
}
</style>
