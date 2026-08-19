import { describe, expect, it } from "vitest";
import { versionLabel } from "./version";

describe("versionLabel", () => {
  it("prefixes a version with a v", () => {
    expect(versionLabel("0.0.5")).toBe("v0.0.5");
  });

  it("stays empty until a version is known", () => {
    expect(versionLabel(null)).toBe("");
    expect(versionLabel("  ")).toBe("");
  });
});
