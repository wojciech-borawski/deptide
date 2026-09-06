import { createRouter, createWebHashHistory } from "vue-router";

import { useWorkspaceStore } from "@/stores/workspace";

export const routeNames = {
  workspace: "workspace",
  wizard: "wizard",
  run: "run",
  transfer: "transfer",
  terminal: "terminal",
  history: "history",
  settings: "settings",
} as const;

export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    {
      path: "/",
      name: routeNames.workspace,
      component: () => import("@/views/WorkspaceView.vue"),
    },
    {
      path: "/wizard",
      name: routeNames.wizard,
      component: () => import("@/views/WizardView.vue"),
    },
    {
      path: "/run",
      name: routeNames.run,
      component: () => import("@/views/RunView.vue"),
    },
    {
      path: "/transfer",
      name: routeNames.transfer,
      component: () => import("@/views/TransferView.vue"),
    },
    {
      path: "/terminal",
      name: routeNames.terminal,
      component: () => import("@/views/TerminalView.vue"),
    },
    {
      path: "/history",
      name: routeNames.history,
      component: () => import("@/views/HistoryView.vue"),
    },
    {
      path: "/settings",
      name: routeNames.settings,
      component: () => import("@/views/SettingsView.vue"),
    },
  ],
});

router.beforeEach((to) => {
  const workspace = useWorkspaceStore();

  if (to.name !== routeNames.workspace && !workspace.isOpen) {
    return { name: routeNames.workspace };
  }

  return true;
});
