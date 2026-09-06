import { computed, ref, watch } from "vue";
import { useLocalStorage } from "@vueuse/core";
import { defineStore } from "pinia";

import * as api from "@/api/commands";
import { useAsyncAction } from "@/composables/useAsyncAction";
import type { AppInfo, ProjectView, UpdateInfo } from "@/api/types";
import { detectLocale, isLocale, setLocale, type Locale } from "@/i18n";

export type Theme = "system" | "dark" | "light";

export type Density = "comfortable" | "compact";

export type PathDisplay = "absolute" | "relative";

interface StoredPreferences {
  theme: Theme;
  density: Density;
  locale: Locale;
  updateUrl: string;
  pathDisplay: PathDisplay;
}

const storageKey = "deptide:preferences";

const themes: readonly Theme[] = ["system", "dark", "light"];

const densities: readonly Density[] = ["comfortable", "compact"];

const pathDisplays: readonly PathDisplay[] = ["absolute", "relative"];

function oneOf<T extends string>(allowed: readonly T[], value: unknown, fallback: T): T {
  return allowed.includes(value as T) ? (value as T) : fallback;
}

function parsePreferences(raw: string): StoredPreferences {
  const defaults: StoredPreferences = {
    theme: "system",
    density: "comfortable",
    locale: detectLocale(),
    updateUrl: "",
    pathDisplay: "absolute",
  };

  try {
    const parsed = JSON.parse(raw) as Partial<StoredPreferences>;
    return {
      theme: oneOf(themes, parsed.theme, defaults.theme),
      density: oneOf(densities, parsed.density, defaults.density),
      locale: typeof parsed.locale === "string" && isLocale(parsed.locale) ? parsed.locale : defaults.locale,
      updateUrl: typeof parsed.updateUrl === "string" ? parsed.updateUrl : defaults.updateUrl,
      pathDisplay: oneOf(pathDisplays, parsed.pathDisplay, defaults.pathDisplay),
    };
  } catch {
    return defaults;
  }
}

function applyTheme(theme: Theme): void {
  const root = document.documentElement;
  if (theme === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", theme);
}

function applyDensity(density: Density): void {
  document.documentElement.setAttribute("data-density", density);
}

export const useUiStore = defineStore("ui", () => {
  const preferences = useLocalStorage<StoredPreferences>(storageKey, parsePreferences(""), {
    serializer: { read: parsePreferences, write: JSON.stringify },
  });

  const theme = computed({ get: () => preferences.value.theme, set: (value) => (preferences.value.theme = value) });
  const density = computed({
    get: () => preferences.value.density,
    set: (value) => (preferences.value.density = value),
  });
  const locale = computed({ get: () => preferences.value.locale, set: (value) => (preferences.value.locale = value) });
  const updateUrl = computed({
    get: () => preferences.value.updateUrl,
    set: (value) => (preferences.value.updateUrl = value),
  });
  const pathDisplay = computed({
    get: () => preferences.value.pathDisplay,
    set: (value) => (preferences.value.pathDisplay = value),
  });

  const update = ref<UpdateInfo | null>(null);
  const info = ref<AppInfo>({ name: "Deptide", version: "", identifier: "dev.deptide.app" });
  const updateAction = useAsyncAction();
  const updateError = updateAction.error;
  const checkingUpdate = updateAction.busy;

  function displayPath(project: Pick<ProjectView, "absolutePath" | "path">): string {
    return pathDisplay.value === "absolute" ? project.absolutePath : project.path;
  }

  function applyAppearance(): void {
    applyTheme(theme.value);
    applyDensity(density.value);
    setLocale(locale.value);
  }

  async function loadInfo(): Promise<void> {
    try {
      info.value = await api.appInfo();
    } catch {
      return;
    }
  }

  async function checkForUpdate(): Promise<void> {
    const url = updateUrl.value.trim();
    if (!url) {
      update.value = null;
      return;
    }

    await updateAction.run(async () => {
      update.value = await api.checkForUpdate(url);
    });
  }

  watch([theme, density, locale], applyAppearance, { immediate: true });

  return {
    theme,
    density,
    locale,
    updateUrl,
    pathDisplay,
    update,
    info,
    updateError,
    checkingUpdate,
    displayPath,
    loadInfo,
    checkForUpdate,
  };
});
