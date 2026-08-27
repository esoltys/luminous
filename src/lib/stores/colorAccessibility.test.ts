import { describe, it, expect } from "vitest";
import { PREDEFINED_THEMES, LUMINOUS_DARK_COLORS, LUMINOUS_LIGHT_COLORS } from "./theme.svelte";
import { checkWcagCompliance, pickAccessibleOnColor } from "../utils/colorUtils";

describe("PREDEFINED_THEMES", () => {
  it("has dynamic-artwork (✨ Luminous) as the first theme in the list", () => {
    expect(PREDEFINED_THEMES[0].id).toBe("dynamic-artwork");
    expect(PREDEFINED_THEMES[0].name).toBe("✨ Luminous");
  });

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

const rubyRedColors = PREDEFINED_THEMES.find(t => t.id === "ruby-red")!.colors;
const nordicBlueColors = PREDEFINED_THEMES.find(t => t.id === "nordic-blue")!.colors;
const retroAmberColors = PREDEFINED_THEMES.find(t => t.id === "retro-amber")!.colors;
const metallicColors = PREDEFINED_THEMES.find(t => t.id === "metallic")!.colors;
const sabrinaColors = PREDEFINED_THEMES.find(t => t.id === "sabrina")!.colors;

describe.each([
  ["dark", LUMINOUS_DARK_COLORS],
  ["light", LUMINOUS_LIGHT_COLORS],
  ["Ruby Red (generated)", rubyRedColors],
  ["Nordic Blue (generated)", nordicBlueColors],
  ["Retro Amber (generated)", retroAmberColors],
  ["Metallic", metallicColors],
  ["Sabrina", sabrinaColors]
] as const)("%s palette accessibility", (_scheme, palette) => {
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
    const result = checkWcagCompliance(LUMINOUS_LIGHT_COLORS["color-accent"], LUMINOUS_LIGHT_COLORS["bg-main"]);
    expect(result.ratio).toBeGreaterThanOrEqual(3);
  });
});

describe("on-accent text contrast (heuristically derived, not hand-picked)", () => {
  const themesWithLiteralAccent = PREDEFINED_THEMES.filter(t => t.id !== "dynamic-artwork");

  // pickAccessibleOnColor now picks by perceived brightness (YIQ) rather
  // than by whichever of white/black has the marginally higher WCAG ratio
  // — for medium-saturation blues/teals (e.g. the shared Luminous accent,
  // #6f7ea9) the old ratio-only logic picked black because it edged out
  // white 5.2:1 to 4.0:1, but that "win" still read as muddy dark-on-dark
  // on screen. Perceptual brightness gets this right at the cost of the
  // strict 4.5:1 text minimum for those near-boundary colors, so the floor
  // checked here is WCAG 1.4.11's 3:1 non-text/large-text threshold instead.
  it.each(themesWithLiteralAccent.map(t => [t.name, t.colors["color-accent"]] as const))(
    "%s: picks a text color that clears the 3:1 non-text threshold against its own accent",
    (_name, accent) => {
      const onColor = pickAccessibleOnColor(accent);
      expect(checkWcagCompliance(onColor, accent).ratio).toBeGreaterThanOrEqual(3);
    }
  );

  it.each([
    ["Luminous dark", LUMINOUS_DARK_COLORS["color-accent"]],
    ["Luminous light", LUMINOUS_LIGHT_COLORS["color-accent"]]
  ])("%s: picks a text color that clears the 3:1 non-text threshold against its own accent", (_name, accent) => {
    const onColor = pickAccessibleOnColor(accent);
    expect(checkWcagCompliance(onColor, accent).ratio).toBeGreaterThanOrEqual(3);
  });

  it("picks white for a dark accent and black for a light accent", () => {
    expect(pickAccessibleOnColor("#1a1a2e")).toBe("#ffffff");
    expect(pickAccessibleOnColor("#f5f5f5")).toBe("#000000");
  });

  it("picks white (perceived-brightness) rather than black (marginal WCAG-ratio winner) for the shared Luminous accent", () => {
    expect(pickAccessibleOnColor(LUMINOUS_LIGHT_COLORS["color-accent"])).toBe("#ffffff");
  });
});
