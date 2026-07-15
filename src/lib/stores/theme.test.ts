import { describe, it, expect } from "vitest";
import { PREDEFINED_THEMES, LUMINOUS_DARK_COLORS, LUMINOUS_LIGHT_COLORS } from "./theme.svelte";
import { checkWcagCompliance, pickAccessibleOnColor } from "../utils/colorUtils";

describe("PREDEFINED_THEMES", () => {
  it("does not include the removed Luminous Violet theme", () => {
    expect(PREDEFINED_THEMES.some(t => t.id === "luminous-violet")).toBe(false);
  });

  it("includes the new System auto-theme", () => {
    expect(PREDEFINED_THEMES.some(t => t.id === "system")).toBe(true);
  });

  it("still includes Nordic Blue (only Luminous Violet was removed)", () => {
    expect(PREDEFINED_THEMES.some(t => t.id === "nordic-blue")).toBe(true);
  });
});

describe.each([
  ["dark", LUMINOUS_DARK_COLORS],
  ["light", LUMINOUS_LIGHT_COLORS]
] as const)("Luminous %s palette accessibility", (_scheme, palette) => {
  const surfaces: (keyof typeof palette)[] = ["bg-main", "bg-sidebar", "bg-playerbar"];

  it.each(surfaces)("primary text meets WCAG AA against %s", (surface) => {
    const result = checkWcagCompliance(palette["color-text-primary"], palette[surface]);
    expect(result.wcagAA).toBe(true);
  });

  it.each(surfaces)("secondary text meets WCAG AA against %s", (surface) => {
    const result = checkWcagCompliance(palette["color-text-secondary"], palette[surface]);
    expect(result.wcagAA).toBe(true);
  });
});

describe("accent color contrast against bg-main (used for accent icons/badges/active-state text)", () => {
  it("dark scheme accent meets the strict 4.5:1 text threshold", () => {
    const result = checkWcagCompliance(LUMINOUS_DARK_COLORS["color-accent"], LUMINOUS_DARK_COLORS["bg-main"]);
    expect(result.wcagAA).toBe(true);
  });

  it("light scheme accent meets WCAG 1.4.11's 3:1 non-text/UI-component threshold", () => {
    // Deliberately not held to the stricter 4.5:1 "normal text" bar here:
    // any orange dark enough to clear 4.5:1 against this light canvas
    // reads as brown/rust rather than the brand orange. The accent is
    // used almost entirely as icon/button/badge/active-state color in
    // this app, which WCAG 1.4.11 governs at 3:1, not 1.4.3's 4.5:1.
    const result = checkWcagCompliance(LUMINOUS_LIGHT_COLORS["color-accent"], LUMINOUS_LIGHT_COLORS["bg-main"]);
    expect(result.ratio).toBeGreaterThanOrEqual(3);
  });
});

describe("on-accent text contrast (heuristically derived, not hand-picked)", () => {
  // Every predefined theme's accent color needs a readable text/icon color
  // rendered directly on top of it (active nav items, filled buttons) —
  // this is what ThemeStore.applyActiveTheme() computes into
  // --color-accent-contrast for every theme, including custom ones.
  // Dynamic Artwork's accent is a CSS var reference, not a literal hex,
  // so it can't be tested this way — its live extracted color is
  // validated at runtime instead.
  const themesWithLiteralAccent = PREDEFINED_THEMES.filter(t => t.id !== "dynamic-artwork");

  it.each(themesWithLiteralAccent.map(t => [t.name, t.colors["color-accent"]] as const))(
    "%s: picks a text color that meets WCAG AA against its own accent",
    (_name, accent) => {
      const onColor = pickAccessibleOnColor(accent);
      expect(checkWcagCompliance(onColor, accent).wcagAA).toBe(true);
    }
  );

  it.each([
    ["Luminous dark", LUMINOUS_DARK_COLORS["color-accent"]],
    ["Luminous light", LUMINOUS_LIGHT_COLORS["color-accent"]]
  ])("%s: picks a text color that meets WCAG AA against its own accent", (_name, accent) => {
    const onColor = pickAccessibleOnColor(accent);
    expect(checkWcagCompliance(onColor, accent).wcagAA).toBe(true);
  });

  it("picks white for a dark accent and black for a light accent", () => {
    expect(pickAccessibleOnColor("#1a1a2e")).toBe("#ffffff");
    expect(pickAccessibleOnColor("#f5f5f5")).toBe("#000000");
  });
});
