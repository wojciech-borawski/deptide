import { describe, expect, it } from "vitest";

import { de } from "@/i18n/de";
import { en } from "@/i18n/en";
import { pl } from "@/i18n/pl";

function flatten(value: unknown, prefix = ""): string[] {
  if (typeof value === "string") return [prefix];
  if (!value || typeof value !== "object") return [];

  return Object.entries(value as Record<string, unknown>).flatMap(([key, child]) =>
    flatten(child, prefix ? `${prefix}.${key}` : key),
  );
}

function read(value: unknown, path: string): string {
  return path.split(".").reduce<unknown>((node, key) => (node as Record<string, unknown>)[key], value) as string;
}

function placeholders(text: string): string[] {
  return [...new Set([...text.matchAll(/\{(\w+)\}/g)].map((match) => match[1] ?? ""))].sort();
}

const englishKeys = flatten(en).sort();

describe("translations", () => {
  it.each([
    ["pl", pl],
    ["de", de],
  ])("%s covers every English key with the same placeholders", (_name, catalogue) => {
    expect(flatten(catalogue).sort()).toEqual(englishKeys);

    for (const key of englishKeys) {
      expect(placeholders(read(catalogue, key)), key).toEqual(placeholders(read(en, key)));
    }
  });

  it("keeps plural forms for the nouns that are pluralised", () => {
    for (const key of ["common.project", "common.package", "common.library", "common.dependency", "common.line"]) {
      expect(read(en, key).split("|").length).toBe(2);
      expect(read(de, key).split("|").length).toBe(2);
      expect(read(pl, key).split("|").length).toBe(3);
    }
  });

  it("has a diagnosis text for every backend code", () => {
    for (const code of [
      "ERESOLVE",
      "E401",
      "E404",
      "ETARGET",
      "EINTEGRITY",
      "ENETWORK",
      "ELOCKED",
      "ELOCKFILE",
      "TSERROR",
      "EMODULE",
      "ENOMEM",
    ]) {
      expect(read(en, `diagnosis.${code}.title`)).toBeTruthy();
      expect(read(en, `diagnosis.${code}.hint`)).toBeTruthy();
    }
  });
});
