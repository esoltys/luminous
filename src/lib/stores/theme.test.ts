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

  it("accent color meets WCAG AA against bg-main for accent text/icons", () => {
    const result = checkWcagCompliance(palette["color-accent"], palette["bg-main"]);
    expect(result.wcagAA).toBe(true);
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
