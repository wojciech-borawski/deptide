import { beforeEach, describe, expect, it, vi } from "vitest";

import { parsePreferences } from "@/stores/ui";

beforeEach(() => {
  vi.stubGlobal("navigator", { language: "en-US" });
});

describe("parsePreferences receiveSplit", () => {
  it("defaults to 32 percent when nothing is stored", () => {
    expect(parsePreferences("").receiveSplit).toBe(32);
    expect(parsePreferences("{}").receiveSplit).toBe(32);
  });

  it("keeps a stored split inside the allowed range", () => {
    expect(parsePreferences(JSON.stringify({ receiveSplit: 45.5 })).receiveSplit).toBe(45.5);
    expect(parsePreferences(JSON.stringify({ receiveSplit: 5 })).receiveSplit).toBe(5);
    expect(parsePreferences(JSON.stringify({ receiveSplit: 95 })).receiveSplit).toBe(95);
  });

  it("falls back to the default for anything that is not a sane percentage", () => {
    for (const receiveSplit of [4.9, 95.1, 0, -10, 1000, "40", null, true, {}]) {
      expect(parsePreferences(JSON.stringify({ receiveSplit })).receiveSplit).toBe(32);
    }
  });
});
