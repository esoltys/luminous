import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { LOCALES, MANUAL_LANGUAGES, legalDocUrl, manualLanguageForLocale, resolveLocale } from "./index";

describe("resolveLocale", () => {
  it("prefers the exact locale", () => {
    expect(resolveLocale(["fr-CA"])).toBe("fr-CA");
    expect(resolveLocale(["en-GB"])).toBe("en-GB");
    expect(resolveLocale(["EN-us"])).toBe("en-US");
  });

  it("falls back to the bare language, then to the first locale of that language", () => {
    expect(resolveLocale(["fr-FR"])).toBe("fr");
    expect(resolveLocale(["fr-BE"])).toBe("fr");
    expect(resolveLocale(["uk-UA"])).toBe("uk");
    expect(resolveLocale(["en-AU"])).toBe("en-CA");
  });

  it("takes the first preference that matches anything", () => {
    expect(resolveLocale(["ja-JP", "it-IT", "de"])).toBe("it");
  });

  it("gives the base locale for an empty or unsupported list", () => {
    expect(resolveLocale([])).toBe("en-CA");
    expect(resolveLocale(["ja", "zh-Hans"])).toBe("en-CA");
    expect(resolveLocale([""])).toBe("en-CA");
  });
});

describe("legal documents", () => {
  const docs = [
    { doc: "terms", file: "TERMS.md" },
    { doc: "privacy", file: "PRIVACY.md" },
  ] as const;

  it("links each locale to its language's section", () => {
    expect(legalDocUrl("terms", "en-US")).toBe("https://github.com/esoltys/luminous/blob/main/TERMS.md#english");
    expect(legalDocUrl("privacy", "fr")).toBe("https://github.com/esoltys/luminous/blob/main/PRIVACY.md#fran%C3%A7ais");
    expect(legalDocUrl("terms", "uk")).toBe(
      "https://github.com/esoltys/luminous/blob/main/TERMS.md#" + encodeURIComponent("українська")
    );
  });

  for (const { doc, file } of docs) {
    const text = readFileSync(resolve(__dirname, "../../..", file), "utf-8");
    const sections = text.split(/^## /m).slice(1);

    it(`${file} has one section per language, matching the anchors the app links to`, () => {
      const anchors = sections.map((s) => s.split("\n")[0].trim().toLowerCase());
      const expected = LOCALES.map((def) => decodeURIComponent(legalDocUrl(doc, def.tag).split("#")[1]));
      expect(new Set(anchors)).toEqual(new Set(expected));
      expect(anchors).toHaveLength(MANUAL_LANGUAGES.length);
      expect(new Set(LOCALES.map((def) => manualLanguageForLocale(def.tag)))).toEqual(new Set(MANUAL_LANGUAGES));
    });

    it(`${file} sections share one structure`, () => {
      const shape = (s: string) => s.split("\n").filter((l) => /^#{3,4} /.test(l)).map((l) => l.match(/^#+/)![0] + (l.match(/^#### (\d+)\./)?.[1] ?? ""));
      const [first, ...rest] = sections.map(shape);
      for (const other of rest) expect(other).toEqual(first);
    });
  }
});
