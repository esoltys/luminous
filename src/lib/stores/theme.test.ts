import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  PREDEFINED_THEMES,
  LUMINOUS_DARK_COLORS,
  LUMINOUS_LIGHT_COLORS,
  buildExtractedColors,
  ThemeStore,
  blendToward,
  hexToRgbaString,
  extractColorsFromImage,
  type Theme
} from "./theme.svelte";
import { checkWcagCompliance, pickAccessibleOnColor, hexToRgb, rgbToHsl, hslToRgb, rgbToHex } from "../utils/colorUtils";
import { invoke } from "@tauri-apps/api/core";

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

const rubyRedColors = PREDEFINED_THEMES.find(t => t.id === "ruby-red")!.colors;
const nordicBlueColors = PREDEFINED_THEMES.find(t => t.id === "nordic-blue")!.colors;
const retroAmberColors = PREDEFINED_THEMES.find(t => t.id === "retro-amber")!.colors;

describe.each([
  ["dark", LUMINOUS_DARK_COLORS],
  ["light", LUMINOUS_LIGHT_COLORS],
  ["Ruby Red (generated)", rubyRedColors],
  ["Nordic Blue (generated)", nordicBlueColors],
  ["Retro Amber (generated)", retroAmberColors]
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

describe("generated predefined themes' accent contrast against bg-main (WCAG 1.4.11 3:1 non-text threshold)", () => {
  it.each([
    ["Ruby Red", rubyRedColors],
    ["Nordic Blue", nordicBlueColors],
    ["Retro Amber", retroAmberColors]
  ] as const)("%s", (_name, colors) => {
    const result = checkWcagCompliance(colors["color-accent"], colors["bg-main"]);
    expect(result.ratio).toBeGreaterThanOrEqual(3);
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

describe("buildExtractedColors (archetype-based artwork color extraction, #61)", () => {
  const darkCoverWithNeonAccent = [
    { r: 5, g: 5, b: 5, count: 1000 },
    { r: 20, g: 40, b: 255, count: 5 }
  ];

  it("picks the small neon cluster as the accent instead of losing it to the black background", () => {
    const colors = buildExtractedColors(darkCoverWithNeonAccent);
    const accentRgb = hexToRgb(colors.accent);
    expect(accentRgb.b).toBeGreaterThan(150);
  });

  it("keeps the primary background dark enough for the fixed Dynamic Artwork text colors", () => {
    const colors = buildExtractedColors(darkCoverWithNeonAccent);
    expect(checkWcagCompliance("#ffffff", colors.primary).wcagAA).toBe(true);
    expect(checkWcagCompliance("#e2e8f0", colors.primary).wcagAA).toBe(true);
  });

  it("keeps sidebar/playerbar darker than, and border lighter than, the primary background", () => {
    const colors = buildExtractedColors([{ r: 80, g: 40, b: 160, count: 1000 }]);
    const luminanceOf = (hex: string) => checkWcagCompliance("#000000", hex).ratio;
    expect(luminanceOf(colors.sidebar)).toBeLessThanOrEqual(luminanceOf(colors.primary));
    expect(luminanceOf(colors.playerbar)).toBeLessThanOrEqual(luminanceOf(colors.primary));
    expect(luminanceOf(colors.border)).toBeGreaterThanOrEqual(luminanceOf(colors.primary));
  });

  it("keeps the accent in a visible lightness range even for a fully desaturated dominant color", () => {
    const colors = buildExtractedColors([{ r: 8, g: 8, b: 8, count: 1000 }]);
    const rgb = hexToRgb(colors.accent);
    const hsl = rgbToHsl(rgb.r, rgb.g, rgb.b);
    expect(hsl.l).toBeGreaterThanOrEqual(0.3);
  });
});

describe("HSL & Color Utilities", () => {
  it("blendToward correctly blends hex toward white and black", () => {
    const whiteBlended = blendToward("#000000", 255, 0.5);
    expect(whiteBlended.toLowerCase()).toBe("#808080");

    const blackBlended = blendToward("#ffffff", 0, 0.5);
    expect(blackBlended.toLowerCase()).toBe("#808080");
  });

  it("hexToRgbaString generates valid rgba strings", () => {
    expect(hexToRgbaString("#ff0000", 0.5)).toBe("rgba(255, 0, 0, 0.5)");
    expect(hexToRgbaString("#00ff00", 1)).toBe("rgba(0, 255, 0, 1)");
  });

  it("rgbToHsl and hslToRgb accurately roundtrip primary colors", () => {
    const pureRedHsl = rgbToHsl(255, 0, 0);
    expect(pureRedHsl.h).toBeCloseTo(0);
    expect(pureRedHsl.s).toBeCloseTo(1);
    expect(pureRedHsl.l).toBeCloseTo(0.5);

    const pureRedRgb = hslToRgb(pureRedHsl.h, pureRedHsl.s, pureRedHsl.l);
    expect(pureRedRgb).toEqual({ r: 255, g: 0, b: 0 });
  });
});

describe("Custom Theme Builder & ThemeStore", () => {
  let themeStore: ThemeStore;

  beforeEach(() => {
    vi.clearAllMocks();
    themeStore = new ThemeStore();
  });

  it("initializes and loads saved custom themes and active theme ID", async () => {
    const mockCustomTheme: Theme = {
      id: "custom-neon",
      name: "Custom Neon",
      colors: { ...LUMINOUS_DARK_COLORS, "color-accent": "#00ff00" },
      isCustom: true
    };

    vi.mocked(invoke).mockResolvedValueOnce({
      custom_themes: JSON.stringify([mockCustomTheme]),
      active_theme_id: "custom-neon"
    } as any);

    await themeStore.init();

    expect(themeStore.customThemes).toHaveLength(1);
    expect(themeStore.customThemes[0].id).toBe("custom-neon");
    expect(themeStore.activeThemeId).toBe("custom-neon");
    expect(themeStore.currentTheme.name).toBe("Custom Neon");
  });

  it("adds and updates a custom theme, invoking set_app_setting", async () => {
    const customTheme: Theme = {
      id: "my-theme",
      name: "My Theme",
      colors: { ...LUMINOUS_DARK_COLORS, "color-accent": "#ff00ff" },
      isCustom: true
    };

    await themeStore.addCustomTheme(customTheme);

    expect(themeStore.customThemes).toContainEqual(customTheme);
    expect(themeStore.activeThemeId).toBe("my-theme");
    expect(invoke).toHaveBeenCalledWith("set_app_setting", {
      key: "custom_themes",
      value: JSON.stringify([customTheme])
    });
    expect(invoke).toHaveBeenCalledWith("set_app_setting", {
      key: "active_theme_id",
      value: "my-theme"
    });
  });

  it("deletes a custom theme and resets to system theme if it was active", async () => {
    const customTheme: Theme = {
      id: "temp-theme",
      name: "Temp Theme",
      colors: { ...LUMINOUS_DARK_COLORS },
      isCustom: true
    };

    await themeStore.addCustomTheme(customTheme);
    expect(themeStore.activeThemeId).toBe("temp-theme");

    await themeStore.deleteCustomTheme("temp-theme");

    expect(themeStore.customThemes).toHaveLength(0);
    expect(themeStore.activeThemeId).toBe("system");
  });

  it("resolves correct theme colors for system theme depending on systemColorScheme", () => {
    themeStore.activeThemeId = "system";

    themeStore.systemColorScheme = "dark";
    expect(themeStore.resolvedColors).toEqual(LUMINOUS_DARK_COLORS);

    themeStore.systemColorScheme = "light";
    expect(themeStore.resolvedColors).toEqual(LUMINOUS_LIGHT_COLORS);
  });

  it("resolves dynamic artwork colors with fallback when artworkColors is null", () => {
    themeStore.activeThemeId = "dynamic-artwork";
    themeStore.artworkColors = null;

    const colors = themeStore.resolvedColors;
    expect(colors["bg-main"]).toBe("#2e3440");
    expect(colors["color-accent"]).toBe("#88c0d0");
  });

  it("switching to Dynamic Artwork applies already-cached artwork colors immediately, not just on the next track change", async () => {
    // Simulate a song already playing (and its colors already extracted)
    // while some other theme is active — updateArtworkColors() caches
    // artworkColors regardless of the active theme.
    themeStore.activeThemeId = "nordic-blue";
    themeStore.artworkColors = {
      primary: "#123456",
      sidebar: "#234567",
      playerbar: "#345678",
      accent: "#456789",
      accentHover: "#56789a",
      border: "#6789ab"
    };

    await themeStore.setTheme("dynamic-artwork");

    expect(document.documentElement.style.getPropertyValue("--color-artwork-primary")).toBe("#123456");
    expect(document.documentElement.style.getPropertyValue("--color-artwork-accent")).toBe("#456789");
  });
});

describe("Image Extraction Fallbacks", () => {
  it("extractColorsFromImage returns fallback colors when image fails to load", async () => {
    class MockImage {
      crossOrigin = "";
      onerror: (() => void) | null = null;
      onload: (() => void) | null = null;
      set src(_url: string) {
        setTimeout(() => this.onerror?.(), 0);
      }
    }

    vi.stubGlobal("Image", MockImage);

    const colors = await extractColorsFromImage("invalid-image-url.jpg");
    expect(colors.primary).toBe("#2e3440");
    expect(colors.accent).toBe("#88c0d0");

    vi.unstubAllGlobals();
  });

  it("updateArtworkColors resets or clears artwork colors when song is undefined or art unavailable", async () => {
    const themeStore = new ThemeStore();

    await themeStore.updateArtworkColors(undefined);
    expect(themeStore.artworkColors).toBeNull();

    themeStore.activeThemeId = "dynamic-artwork";
    await themeStore.updateArtworkColors(undefined);
    expect(themeStore.artworkColors).toBeNull();
    expect(themeStore.resolvedColors["bg-main"]).toBe("#2e3440");
  });
});

