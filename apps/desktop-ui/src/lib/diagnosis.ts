import type { Diagnosis } from "@/api/types";

export interface Translator {
  (key: string, params?: Record<string, unknown>): string;
}

export interface KeyChecker {
  (key: string): boolean;
}

export function localizeDiagnosis(
  diagnosis: Diagnosis,
  t: Translator,
  exists: KeyChecker,
): { title: string; hint: string } {
  const titleKey = `diagnosis.${diagnosis.code}.title`;
  const hintKey = `diagnosis.${diagnosis.code}.hint`;

  return {
    title: exists(titleKey) ? t(titleKey) : diagnosis.title,
    hint: exists(hintKey) ? t(hintKey) : diagnosis.hint,
  };
}

export function shortHash(integrity: string | null): string {
  if (!integrity) return "";
  const digest = integrity.split("-")[1] ?? integrity;
  return digest.slice(0, 10);
}
