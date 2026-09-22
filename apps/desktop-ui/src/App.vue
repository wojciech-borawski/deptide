<script setup lang="ts">
import { onMounted } from "vue";
import { useRouter } from "vue-router";

import { startupWorkspace } from "@/api/commands";
import AppRail from "@/components/layout/AppRail.vue";
import ToastHost from "@/components/ui/ToastHost.vue";
import { routeNames } from "@/router";
import { useRunStore } from "@/stores/run";
import { useTerminalStore } from "@/stores/terminal";
import { useUiStore } from "@/stores/ui";
import { useWorkspaceStore } from "@/stores/workspace";

const workspace = useWorkspaceStore();
const run = useRunStore();
const terminal = useTerminalStore();
const ui = useUiStore();
const router = useRouter();

onMounted(async () => {
  await run.subscribe();
  await terminal.subscribe();
  await workspace.loadRecent();
  await ui.loadInfo();
  void ui.checkForUpdate();

  const initial = (await startupWorkspace().catch(() => null)) ?? workspace.lastWorkspace;
  if (initial && (await workspace.open(initial))) {
    await router.replace({ name: routeNames.wizard });
  }
});
</script>

<template>
  <div class="shell">
    <AppRail />
    <main class="content">
      <router-view v-slot="{ Component }">
        <transition name="fade" mode="out-in">
          <component :is="Component" />
        </transition>
      </router-view>
    </main>
    <ToastHost />
  </div>
</template>

<style scoped>
.shell {
  display: grid;
  grid-template-columns: var(--rail-width) 1fr;
  height: 100%;
}

.content {
  min-width: 0;
  height: 100%;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}
</style>
