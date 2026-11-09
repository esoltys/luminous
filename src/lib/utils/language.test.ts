import { describe, it, expect } from "vitest";
import { getLanguageDisplayName } from "./language";

describe("getLanguageDisplayName", () => {
  it("resolves ISO 639-2 codes to localized names", () => {
    expect(getLanguageDisplayName("eng")).toBe("English");
    expect(getLanguageDisplayName("jpn")).toBe("Japanese");
    expect(getLanguageDisplayName("fra")).toBe("French");
    expect(getLanguageDisplayName("deu")).toBe("German");
    expect(getLanguageDisplayName("spa")).toBe("Spanish");
  });

  it("resolves ISO 639-1 codes to localized names", () => {
    expect(getLanguageDisplayName("en")).toBe("English");
    expect(getLanguageDisplayName("ja")).toBe("Japanese");
    expect(getLanguageDisplayName("fr")).toBe("French");
  });

  it("handles null, undefined, empty, or whitespace", () => {
    expect(getLanguageDisplayName(null)).toBe("");
    expect(getLanguageDisplayName(undefined)).toBe("");
    expect(getLanguageDisplayName("")).toBe("");
    expect(getLanguageDisplayName("   ")).toBe("");
  });

  it("falls back to the raw code for unrecognized codes", () => {
    expect(getLanguageDisplayName("xyz123")).toBe("xyz123");
  });
});
