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

export type ReceiveLayout = "list" | "tree";

export type ReceiveSort = "path" | "change";

export type DiffMode = "unified" | "split";

interface StoredPreferences {
  theme: Theme;
  density: Density;
  locale: Locale;
  updateUrl: string;
  pathDisplay: PathDisplay;
  receiveLayout: ReceiveLayout;
  receiveOnlyAffected: boolean;
  receiveSort: ReceiveSort;
  receiveSplit: number;
  diffMode: DiffMode;
}

const storageKey = "deptide:preferences";

const themes: readonly Theme[] = ["system", "dark", "light"];

const densities: readonly Density[] = ["comfortable", "compact"];

const pathDisplays: readonly PathDisplay[] = ["absolute", "relative"];

const receiveLayouts: readonly ReceiveLayout[] = ["list", "tree"];

const receiveSorts: readonly ReceiveSort[] = ["path", "change"];

const diffModes: readonly DiffMode[] = ["unified", "split"];

const receiveSplitRange = { min: 5, max: 95 };

function oneOf<T extends string>(allowed: readonly T[], value: unknown, fallback: T): T {
  return allowed.includes(value as T) ? (value as T) : fallback;
}

function inRange(value: unknown, range: { min: number; max: number }, fallback: number): number {
  return typeof value === "number" && value >= range.min && value <= range.max ? value : fallback;
}

export function parsePreferences(raw: string): StoredPreferences {
  const defaults: StoredPreferences = {
    theme: "system",
    density: "comfortable",
    locale: detectLocale(),
    updateUrl: "",
    pathDisplay: "absolute",
    receiveLayout: "list",
    receiveOnlyAffected: true,
    receiveSort: "path",
    receiveSplit: 32,
    diffMode: "unified",
  };

  try {
    const parsed = JSON.parse(raw) as Partial<StoredPreferences>;
    return {
      theme: oneOf(themes, parsed.theme, defaults.theme),
      density: oneOf(densities, parsed.density, defaults.density),
      locale: typeof parsed.locale === "string" && isLocale(parsed.locale) ? parsed.locale : defaults.locale,
      updateUrl: typeof parsed.updateUrl === "string" ? parsed.updateUrl : defaults.updateUrl,
      pathDisplay: oneOf(pathDisplays, parsed.pathDisplay, defaults.pathDisplay),
      receiveLayout: oneOf(receiveLayouts, parsed.receiveLayout, defaults.receiveLayout),
      receiveOnlyAffected:
        typeof parsed.receiveOnlyAffected === "boolean" ? parsed.receiveOnlyAffected : defaults.receiveOnlyAffected,
      receiveSort: oneOf(receiveSorts, parsed.receiveSort, defaults.receiveSort),
      receiveSplit: inRange(parsed.receiveSplit, receiveSplitRange, defaults.receiveSplit),
      diffMode: oneOf(diffModes, parsed.diffMode, defaults.diffMode),
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
  const receiveLayout = computed({
    get: () => preferences.value.receiveLayout,
    set: (value) => (preferences.value.receiveLayout = value),
  });
  const receiveOnlyAffected = computed({
    get: () => preferences.value.receiveOnlyAffected,
    set: (value) => (preferences.value.receiveOnlyAffected = value),
  });
  const receiveSort = computed({
    get: () => preferences.value.receiveSort,
    set: (value) => (preferences.value.receiveSort = value),
  });
  const receiveSplit = computed({
    get: () => preferences.value.receiveSplit,
    set: (value) => (preferences.value.receiveSplit = value),
  });
  const diffMode = computed({
    get: () => preferences.value.diffMode,
    set: (value) => (preferences.value.diffMode = value),
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
    receiveLayout,
    receiveOnlyAffected,
    receiveSort,
    receiveSplit,
    diffMode,
    update,
    info,
    updateError,
    checkingUpdate,
    displayPath,
    loadInfo,
    checkForUpdate,
  };
});
