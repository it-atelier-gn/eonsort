import { describe, expect, it } from "vitest";
import { releaseUrl, versionLabel, RELEASES } from "./version";

describe("versionLabel", () => {
  it("prefixes a version with a v", () => {
    expect(versionLabel("0.0.5")).toBe("v0.0.5");
  });

  it("stays empty until a version is known", () => {
    expect(versionLabel(null)).toBe("");
    expect(versionLabel("  ")).toBe("");
  });
});

describe("Pointing at the release the app came from", () => {
  it("links straight to the tag for a known version", () => {
    expect(releaseUrl("0.0.8")).toBe(`${RELEASES}/tag/v0.0.8`);
    expect(releaseUrl(" 1.2.3 ")).toBe(`${RELEASES}/tag/v1.2.3`);
  });

  it("falls back to the list when the version is unknown", () => {
    expect(releaseUrl(null)).toBe(RELEASES);
    expect(releaseUrl("  ")).toBe(RELEASES);
  });
});
