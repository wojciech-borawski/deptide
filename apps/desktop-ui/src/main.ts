import { createApp } from "vue";
import { createPinia } from "pinia";

import App from "./App.vue";
import { reportError } from "./api/commands";
import { i18n } from "./i18n";
import { router } from "./router";
import { useUiStore } from "./stores/ui";
import "./styles/theme.css";
import "./styles/appearance.css";

if (import.meta.env.VITE_MOCK_IPC === "1") {
  const { installMockBackend } = await import("./dev/mock-backend");
  installMockBackend();
}

function describe(reason: unknown): string {
  if (reason instanceof Error) return `${reason.message}\n${reason.stack ?? ""}`;
  return String(reason);
}

window.addEventListener("error", (event) => {
  reportError("window", `${event.message} (${event.filename}:${event.lineno})`);
});

window.addEventListener("unhandledrejection", (event) => {
  reportError("unhandled promise", describe(event.reason));
});

const app = createApp(App);

app.config.errorHandler = (error, _instance, info) => {
  reportError(`vue ${info}`, describe(error));
  console.error(error);
};

app.use(createPinia()).use(i18n).use(router);
useUiStore();
app.mount("#app");
