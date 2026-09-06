import type { PackageSpec } from "@/api/types";

const semverCore = /^\d+\.\d+\.\d+/;

const suffixPattern = /^-?[A-Za-z0-9][A-Za-z0-9.-]*$/;

export function stripPrerelease(version: string): string {
  const trimmed = version.trim();
  return semverCore.exec(trimmed)?.[0] ?? trimmed;
}

export function applySuffix(baseVersion: string, suffix: string): string {
  const cleanedSuffix = suffix.trim().replace(/^-+/, "");
  const base = stripPrerelease(baseVersion);

  return cleanedSuffix ? `${base}-${cleanedSuffix}` : base;
}

export function isValidSuffix(suffix: string): boolean {
  return suffixPattern.test(suffix.trim());
}

export function parseSpec(spec: string): PackageSpec | null {
  const trimmed = spec.trim();
  const separator = trimmed.lastIndexOf("@");
  if (separator <= 0) return null;

  const name = trimmed.slice(0, separator).trim();
  const version = trimmed.slice(separator + 1).trim();
  if (!name || !version) return null;

  return { name, version, saveDev: false };
}

export function formatSpec(spec: PackageSpec): string {
  return `${spec.name}@${spec.version}`;
}
