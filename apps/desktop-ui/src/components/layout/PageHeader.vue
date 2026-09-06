<script setup lang="ts">
const props = defineProps<{ title: string; subtitle?: string }>();
</script>

<template>
  <header class="page-header">
    <div class="titles">
      <h1>{{ props.title }}</h1>
      <p v-if="props.subtitle" class="muted truncate selectable">{{ props.subtitle }}</p>
    </div>
    <div v-if="$slots.default" class="actions"><slot /></div>
  </header>
</template>

<style scoped>
.page-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
  gap: 10px 16px;
  padding: 14px 24px;
  border-bottom: 1px solid var(--border);
  background: var(--bg-panel);
  min-height: var(--topbar-height);
  flex-shrink: 0;
}

.titles {
  min-width: 0;
}

.titles p {
  font-size: 12.5px;
  margin-top: 2px;
}

.actions {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 10px;
  min-width: 0;
}
</style>
