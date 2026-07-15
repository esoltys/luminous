import { describe, it, expect } from "vitest";
import {
  PREDEFINED_THEMES,
  LUMINOUS_DARK_COLORS,
  LUMINOUS_LIGHT_COLORS,
  blendToward,
  makeAccessibleAccent
} from "./theme.svelte";
import { checkWcagCompliance } from "../utils/colorUtils";

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

describe("blendToward", () => {
  it("blends toward white", () => {
    expect(blendToward("#000000", 255, 0.5)).toBe("#808080");
  });

  it("blends toward black", () => {
    expect(blendToward("#ffffff", 0, 0.5)).toBe("#808080");
  });

  it("returns the original color at amount 0", () => {
    expect(blendToward("#336699", 255, 0)).toBe("#336699");
  });
});

describe("makeAccessibleAccent", () => {
  it("returns the color unchanged when it already passes AA", () => {
    // Near-black on white already has very high contrast
    expect(makeAccessibleAccent("#111111", "#ffffff", false)).toBe("#111111");
  });

  it("lightens a too-dark accent toward white until it passes AA on a dark background", () => {
    // A near-black "accent" has almost no contrast against a near-black background
    const result = makeAccessibleAccent("#0a0a0a", "#08090c", true);
    expect(result).not.toBeNull();
    expect(checkWcagCompliance(result!, "#08090c").wcagAA).toBe(true);
  });

  it("darkens a too-light accent toward black until it passes AA on a light background", () => {
    const result = makeAccessibleAccent("#fafafa", "#ffffff", false);
    expect(result).not.toBeNull();
    expect(checkWcagCompliance(result!, "#ffffff").wcagAA).toBe(true);
  });

  it("gives up and returns null for a background it can never contrast against (mid-gray toward the same tone)", () => {
    // Blending gray toward white can never separate itself enough from a
    // background that is itself near-white — should exhaust the search.
    const result = makeAccessibleAccent("#eeeeee", "#f5f5f5", true);
    expect(result).toBeNull();
  });
});
