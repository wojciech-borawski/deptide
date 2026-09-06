import { describe, expect, it } from "vitest";

import { applySuffix, formatSpec, isValidSuffix, parseSpec, stripPrerelease } from "@/lib/versions";

describe("stripPrerelease", () => {
  it("keeps only the semver core", () => {
    expect(stripPrerelease("3.1.0-ABC-123")).toBe("3.1.0");
    expect(stripPrerelease(" 1.2.3 ")).toBe("1.2.3");
  });

  it("returns non-semver input unchanged", () => {
    expect(stripPrerelease("latest")).toBe("latest");
  });
});

describe("applySuffix", () => {
  it("replaces an existing prerelease with the suffix", () => {
    expect(applySuffix("3.1.0-old", "-ABC-1")).toBe("3.1.0-ABC-1");
    expect(applySuffix("3.1.0", "ABC-1")).toBe("3.1.0-ABC-1");
  });

  it("drops an empty suffix", () => {
    expect(applySuffix("3.1.0-old", "  ")).toBe("3.1.0");
  });
});

describe("isValidSuffix", () => {
  it("accepts ticket-like suffixes and rejects spaces", () => {
    expect(isValidSuffix("ABC-123")).toBe(true);
    expect(isValidSuffix("-rc.1")).toBe(true);
    expect(isValidSuffix("LAB 928")).toBe(false);
    expect(isValidSuffix("")).toBe(false);
  });
});

describe("parseSpec", () => {
  it("splits scoped names on the last @", () => {
    expect(parseSpec("@acme/core@3.1.0-ABC-1")).toEqual({
      name: "@acme/core",
      version: "3.1.0-ABC-1",
      saveDev: false,
    });
  });

  it("rejects incomplete specs", () => {
    expect(parseSpec("@acme/core")).toBeNull();
    expect(parseSpec("core@")).toBeNull();
    expect(parseSpec("@1.0.0")).toBeNull();
  });

  it("round-trips through formatSpec", () => {
    const spec = parseSpec("left-pad@1.3.0");
    expect(spec && formatSpec(spec)).toBe("left-pad@1.3.0");
  });
});
