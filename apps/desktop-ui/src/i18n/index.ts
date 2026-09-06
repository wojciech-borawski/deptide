import { Settings as LuxonSettings } from "luxon";
import { createI18n } from "vue-i18n";

import { de } from "./de";
import { en } from "./en";
import { pl } from "./pl";

export const locales = ["en", "pl", "de"] as const;

export type Locale = (typeof locales)[number];

export const localeNames: Record<Locale, string> = {
  en: "English",
  pl: "Polski",
  de: "Deutsch",
};

function polishPlural(choice: number, choicesLength: number): number {
  if (choicesLength < 3) return choice === 1 ? 0 : 1;
  if (choice === 1) return 0;
  const tens = choice % 100;
  const units = choice % 10;
  return units >= 2 && units <= 4 && (tens < 12 || tens > 14) ? 1 : 2;
}

export function isLocale(value: string): value is Locale {
  return (locales as readonly string[]).includes(value);
}

export function detectLocale(): Locale {
  const candidate = navigator.language?.slice(0, 2).toLowerCase() ?? "en";
  return isLocale(candidate) ? candidate : "en";
}

export const i18n = createI18n({
  legacy: false,
  locale: "en",
  fallbackLocale: "en",
  messages: { en, pl, de },
  pluralRules: { pl: polishPlural },
  missingWarn: false,
  fallbackWarn: false,
});

export function setLocale(locale: Locale): void {
  i18n.global.locale.value = locale;
  document.documentElement.lang = locale;
  LuxonSettings.defaultLocale = locale;
}
