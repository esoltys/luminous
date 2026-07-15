import { describe, it, expect } from "vitest";
import { PREDEFINED_THEMES, LUMINOUS_DARK_COLORS, LUMINOUS_LIGHT_COLORS } from "./theme.svelte";
import { checkWcagCompliance } from "../utils/colorUtils";

describe("PREDEFINED_THEMES", () => {
  it("does not include the removed Luminous Violet theme", () => {
    expect(PREDEFINED_THEMES.some(t => t.id === "luminous-violet")).toBe(false);
  });

  it("includes the new Luminous auto-theme", () => {
    expect(PREDEFINED_THEMES.some(t => t.id === "luminous")).toBe(true);
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
