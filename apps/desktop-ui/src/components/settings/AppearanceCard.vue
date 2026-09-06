<script setup lang="ts">
import { useI18n } from "vue-i18n";

import { localeNames, locales } from "@/i18n";
import { useUiStore, type Density, type PathDisplay, type Theme } from "@/stores/ui";

const ui = useUiStore();
const { t } = useI18n();

const themes: Theme[] = ["system", "dark", "light"];
const densities: Density[] = ["comfortable", "compact"];
const pathDisplays: PathDisplay[] = ["absolute", "relative"];
</script>

<template>
  <section class="card stack">
    <div class="card-title">
      <h3>{{ t("settings.appearance") }}</h3>
    </div>
    <div class="grid-3">
      <div class="field">
        <label>{{ t("settings.theme") }}</label>
        <div class="row wrap">
          <button
            v-for="theme in themes"
            :key="theme"
            class="chip"
            :class="{ active: ui.theme === theme }"
            type="button"
            @click="ui.theme = theme"
          >
            {{ t(`settings.themes.${theme}`) }}
          </button>
        </div>
      </div>
      <div class="field">
        <label>{{ t("settings.density") }}</label>
        <div class="row wrap">
          <button
            v-for="density in densities"
            :key="density"
            class="chip"
            :class="{ active: ui.density === density }"
            type="button"
            @click="ui.density = density"
          >
            {{ t(`settings.densities.${density}`) }}
          </button>
        </div>
      </div>
      <div class="field">
        <label>{{ t("settings.paths") }}</label>
        <div class="row wrap">
          <button
            v-for="display in pathDisplays"
            :key="display"
            class="chip"
            :class="{ active: ui.pathDisplay === display }"
            type="button"
            @click="ui.pathDisplay = display"
          >
            {{ t(`settings.pathDisplays.${display}`) }}
          </button>
        </div>
      </div>
      <div class="field">
        <label>{{ t("settings.language") }}</label>
        <div class="row wrap">
          <button
            v-for="locale in locales"
            :key="locale"
            class="chip"
            :class="{ active: ui.locale === locale }"
            type="button"
            @click="ui.locale = locale"
          >
            {{ localeNames[locale] }}
          </button>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.wrap {
  flex-wrap: wrap;
}
</style>
