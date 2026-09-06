import { useI18n } from "vue-i18n";

export type CountedNoun = "project" | "package" | "library" | "dependency" | "line";

export function useCountedNoun() {
  const { t } = useI18n();
  return (noun: CountedNoun, count: number): string => t(`common.${noun}`, { count }, count);
}
