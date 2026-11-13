import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import {
  LUMINOUS_DARK_COLORS,
  LUMINOUS_LIGHT_COLORS,
  buildExtractedColors,
  ThemeStore,
  blendToward,
  hexToRgbaString,
  flatGlassColor,
  CHROME_SATURATION,
  extractColorsFromImage,
  resolveDockGlowColors,
  DOCK_GLOW_DARK_MAX_L,
  DOCK_GLOW_LIGHT_MIN_L,
  type Theme,
} from "./theme.svelte";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { checkWcagCompliance, hexToRgb, rgbToHsl, hslToRgb } from "../utils/colorUtils";
import { invoke } from "@tauri-apps/api/core";

describe("buildExtractedColors (archetype-based artwork color extraction, #61)", () => {
  const darkCoverWithNeonAccent = [
    { r: 5, g: 5, b: 5, count: 1000 },
    { r: 20, g: 40, b: 255, count: 5 },
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

  it("extracts all 6 Android Palette archetype names on buildExtractedColors", () => {
    const colors = buildExtractedColors([{ r: 200, g: 200, b: 200, count: 1000 }]);
    expect(colors.vibrant).toBeTruthy();
    expect(colors.lightVibrant).toBeTruthy();
    expect(colors.darkVibrant).toBeTruthy();
    expect(colors.muted).toBeTruthy();
    expect(colors.lightMuted).toBeTruthy();
    expect(colors.darkMuted).toBeTruthy();
    expect(typeof colors.isLight).toBe("boolean");
  });

  it("detects light album covers and enables Light Glass Mode (isLight = true)", () => {
    const lightColors = buildExtractedColors([{ r: 240, g: 240, b: 245, count: 1000 }]);
    expect(lightColors.isLight).toBe(true);
    expect(checkWcagCompliance("#16181d", lightColors.primary).wcagAA).toBe(true);
  });
});

describe("buildExtractedColors keeps surfaces to one hue (#2051)", () => {
  const hueOf = (hex: string) => {
    const { r, g, b } = hexToRgb(hex);
    return rgbToHsl(r, g, b).h;
  };
  /** Largest circular distance (degrees) between any two hues. */
  const hueSpread = (hexes: string[]) => {
    const hues = hexes.map(hueOf);
    let max = 0;
    for (const a of hues) {
      for (const b of hues) {
        const d = Math.abs(a - b);
        max = Math.max(max, Math.min(d, 360 - d));
      }
    }
    return max;
  };

  // Golden Hour-style covers: a slate blue field plus rust brown, in both polarities.
  const covers = {
    "dark blue + brown": [
      { r: 40, g: 56, b: 96, count: 800 },
      { r: 120, g: 56, b: 24, count: 700 },
      { r: 224, g: 160, b: 64, count: 60 },
    ],
    "light blue + brown": [
      { r: 200, g: 208, b: 224, count: 900 },
      { r: 248, g: 192, b: 112, count: 400 },
      { r: 240, g: 144, b: 64, count: 200 },
      { r: 96, g: 48, b: 16, count: 80 },
    ],
  };

  for (const [name, counts] of Object.entries(covers)) {
    it(`shares the canvas hue across primary, sidebar, playerbar and border (${name})`, () => {
      const c = buildExtractedColors(counts);
      expect(hueSpread([c.primary, c.sidebar, c.playerbar, c.border])).toBeLessThan(8);
    });
  }

  it("still lets the accent take a different hue from the surfaces", () => {
    const c = buildExtractedColors(covers["dark blue + brown"]);
    expect(hueSpread([c.primary, c.accent])).toBeGreaterThan(8);
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
      isCustom: true,
    };

    vi.mocked(invoke).mockResolvedValueOnce({
      custom_themes: JSON.stringify([mockCustomTheme]),
      active_theme_id: "custom-neon",
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
      isCustom: true,
    };

    await themeStore.addCustomTheme(customTheme);

    expect(themeStore.customThemes).toContainEqual(customTheme);
    expect(themeStore.activeThemeId).toBe("my-theme");
    expect(invoke).toHaveBeenCalledWith("set_app_setting", {
      key: "custom_themes",
      value: JSON.stringify([customTheme]),
    });
    expect(invoke).toHaveBeenCalledWith("set_app_setting", {
      key: "active_theme_id",
      value: "my-theme",
    });
  });

  it("deletes a custom theme and resets to system theme if it was active", async () => {
    const customTheme: Theme = {
      id: "temp-theme",
      name: "Temp Theme",
      colors: { ...LUMINOUS_DARK_COLORS },
      isCustom: true,
    };

    await themeStore.addCustomTheme(customTheme);
    expect(themeStore.activeThemeId).toBe("temp-theme");

    await themeStore.deleteCustomTheme("temp-theme");

    expect(themeStore.customThemes).toHaveLength(0);
    expect(themeStore.activeThemeId).toBe("system");
  });

  it("imports a custom theme and adds it to the store", async () => {
    const importedTheme: Theme = {
      id: "custom-imported-123",
      name: "Imported Glow",
      colors: { ...LUMINOUS_DARK_COLORS, "color-accent": "#10b981" },
      isCustom: true,
    };

    vi.mocked(invoke).mockResolvedValueOnce(importedTheme);

    const result = await themeStore.importTheme("/path/to/theme.json");

    expect(invoke).toHaveBeenCalledWith("import_theme", { filePath: "/path/to/theme.json" });
    expect(result).toEqual(importedTheme);
    expect(themeStore.customThemes).toContainEqual(importedTheme);
    expect(themeStore.activeThemeId).toBe("custom-imported-123");
  });

  it("throws error when importing a theme with missing required colors", async () => {
    const invalidTheme = {
      id: "custom-invalid",
      name: "Invalid Theme",
      colors: {
        "bg-main": "#000",
      },
    };

    vi.mocked(invoke).mockResolvedValueOnce(invalidTheme as any);

    await expect(themeStore.importTheme("/path/to/bad.json")).rejects.toThrow(
      "Missing or invalid color",
    );
  });

  it("exports a custom theme by invoking export_theme command", async () => {
    const themeToExport: Theme = {
      id: "export-me",
      name: "Export Me",
      colors: { ...LUMINOUS_DARK_COLORS },
      isCustom: true,
    };

    vi.mocked(invoke).mockResolvedValueOnce(undefined as any);

    await themeStore.exportTheme(themeToExport, "/path/to/exported.json");

    expect(invoke).toHaveBeenCalledWith("export_theme", {
      theme: themeToExport,
      exportPath: "/path/to/exported.json",
    });
  });

  it("resolves correct theme colors for system theme depending on systemColorScheme", () => {
    themeStore.activeThemeId = "system";

    themeStore.systemColorScheme = "dark";
    expect(themeStore.resolvedColors).toEqual(LUMINOUS_DARK_COLORS);

    themeStore.systemColorScheme = "light";
    expect(themeStore.resolvedColors).toEqual(LUMINOUS_LIGHT_COLORS);
  });

  describe("colorSchemeMode (#692)", () => {
    it("defaults to system and follows systemColorScheme when unpinned", () => {
      themeStore.activeThemeId = "system";
      expect(themeStore.colorSchemeMode).toBe("system");

      themeStore.systemColorScheme = "dark";
      expect(themeStore.effectiveColorScheme).toBe("dark");
      expect(themeStore.resolvedColors).toEqual(LUMINOUS_DARK_COLORS);

      themeStore.systemColorScheme = "light";
      expect(themeStore.effectiveColorScheme).toBe("light");
      expect(themeStore.resolvedColors).toEqual(LUMINOUS_LIGHT_COLORS);
    });

    it("pinning to dark resolves LUMINOUS_DARK_COLORS regardless of the OS preference", async () => {
      themeStore.activeThemeId = "system";
      themeStore.systemColorScheme = "light";

      await themeStore.setColorSchemeMode("dark");

      expect(themeStore.effectiveColorScheme).toBe("dark");
      expect(themeStore.resolvedColors).toEqual(LUMINOUS_DARK_COLORS);
      expect(invoke).toHaveBeenCalledWith("set_app_setting", {
        key: "color_scheme_mode",
        value: "dark",
      });
    });

    it("pinning to light resolves LUMINOUS_LIGHT_COLORS regardless of the OS preference", async () => {
      themeStore.activeThemeId = "system";
      themeStore.systemColorScheme = "dark";

      await themeStore.setColorSchemeMode("light");

      expect(themeStore.effectiveColorScheme).toBe("light");
      expect(themeStore.resolvedColors).toEqual(LUMINOUS_LIGHT_COLORS);
    });

    it("setColorSchemeMode does not change activeThemeId away from system", async () => {
      themeStore.activeThemeId = "system";
      await themeStore.setColorSchemeMode("dark");
      expect(themeStore.activeThemeId).toBe("system");
    });

    it("init() restores a persisted color_scheme_mode", async () => {
      vi.mocked(invoke).mockResolvedValueOnce({
        color_scheme_mode: "dark",
      } as any);

      await themeStore.init();

      expect(themeStore.colorSchemeMode).toBe("dark");
    });

    it("ignores an invalid persisted color_scheme_mode", async () => {
      vi.mocked(invoke).mockResolvedValueOnce({
        color_scheme_mode: "not-a-real-mode",
      } as any);

      await themeStore.init();

      expect(themeStore.colorSchemeMode).toBe("system");
    });
  });

  it("resolves dynamic artwork colors with fallback when artworkColors is null", () => {
    themeStore.activeThemeId = "dynamic-artwork";
    themeStore.artworkColors = null;

    const colors = themeStore.resolvedColors;
    expect(colors["bg-main"]).toBe("#2e3440");
    expect(colors["color-accent"]).toBe("#88c0d0");
    // Fallback palette is dark (isLight: false) -> light/white text
    expect(colors["color-text-primary"]).toBe("#ffffff");
    expect(colors["color-text-secondary"]).toBe("#e2e8f0");
  });

  it("resolvedColors text color tracks artworkColors.isLight, matching what applyActiveTheme renders live (#156)", () => {
    themeStore.activeThemeId = "dynamic-artwork";

    themeStore.artworkColors = {
      isLight: true,
      primary: "#eeeeee",
      sidebar: "#f5f5f5",
      playerbar: "#f0f0f0",
      accent: "#123456",
      accentHover: "#234567",
      border: "#dddddd",
    };
    expect(themeStore.resolvedColors["color-text-primary"]).toBe("#16181d");
    expect(themeStore.resolvedColors["color-text-secondary"]).toBe("#5a6072");

    themeStore.artworkColors = {
      isLight: false,
      primary: "#111111",
      sidebar: "#050505",
      playerbar: "#0a0a0a",
      accent: "#123456",
      accentHover: "#234567",
      border: "#222222",
    };
    expect(themeStore.resolvedColors["color-text-primary"]).toBe("#ffffff");
    expect(themeStore.resolvedColors["color-text-secondary"]).toBe("#e2e8f0");
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
      border: "#6789ab",
    };

    await themeStore.setTheme("dynamic-artwork");

    expect(document.documentElement.style.getPropertyValue("--color-artwork-primary")).toBe(
      "#123456",
    );
    expect(document.documentElement.style.getPropertyValue("--color-artwork-accent")).toBe(
      "#456789",
    );
  });

  it("updateArtworkColors updates CSS custom properties unconditionally even when another theme is active", async () => {
    themeStore.activeThemeId = "nordic-blue";
    const colors = {
      primary: "#112233",
      sidebar: "#223344",
      playerbar: "#334455",
      accent: "#445566",
      accentHover: "#556677",
      border: "#667788",
    };

    themeStore.applyArtworkColors(colors);

    expect(document.documentElement.style.getPropertyValue("--color-artwork-primary")).toBe(
      "#112233",
    );
    expect(document.documentElement.style.getPropertyValue("--color-artwork-accent")).toBe(
      "#445566",
    );
  });

  it("applyArtworkColors triggers applyActiveTheme to update glass panel and contrast variables immediately", () => {
    themeStore.activeThemeId = "dynamic-artwork";
    const spy = vi.spyOn(themeStore, "applyActiveTheme");

    themeStore.applyArtworkColors({
      primary: "#112233",
      sidebar: "#223344",
      playerbar: "#334455",
      accent: "#445566",
      accentHover: "#556677",
      border: "#667788",
    });

    expect(spy).toHaveBeenCalled();
  });
});

describe("flatGlassColor (solid stand-in for glass over the flat canvas)", () => {
  it("composites the tint at its alpha over the backdrop", () => {
    // A gray backdrop is unchanged by saturate(), so this is a plain 50/50 mix.
    expect(flatGlassColor("#ffffff", 0.5, "#000000")).toBe("#808080");
    expect(flatGlassColor("#204060", 0.5, "#808080")).toBe("#506070");
  });

  it("saturates a colored backdrop before compositing, like backdrop-filter: saturate(180%)", () => {
    const flat = hexToRgb(flatGlassColor("#000000", 0, "#6040a0"));
    const plain = hexToRgb("#6040a0");
    expect(flat.b - flat.g).toBeGreaterThan(plain.b - plain.g);
  });

  it("matches the default dark theme's first-paint value in app.css", () => {
    expect(
      flatGlassColor(LUMINOUS_DARK_COLORS["bg-sidebar"], 0.5, LUMINOUS_DARK_COLORS["bg-main"]),
    ).toBe("#1e1e1c");
  });

  // The flattened panels only match the live glass beside them (the player
  // bar) while both use the same saturation, so read the real stylesheet.
  it("uses the saturation of the chrome acrylic recipe in app.css", () => {
    const css = readFileSync(resolve(__dirname, "../../app.css"), "utf-8");
    const match = css.match(/--acrylic-chrome-filter:[^;]*saturate\((\d+)%\)/);
    if (!match) throw new Error("--acrylic-chrome-filter with a saturate(N%) missing from app.css");
    expect(Number(match[1]) / 100).toBe(CHROME_SATURATION);
  });
});

describe("Theme change View Transitions", () => {
  let startViewTransition: ReturnType<typeof vi.fn>;
  const fakeTransition = () => ({
    ready: new Promise<void>(() => {}),
    finished: new Promise<void>(() => {}),
    skipTransition: vi.fn(),
  });

  beforeEach(() => {
    startViewTransition = vi.fn((update: () => void) => {
      update();
      return fakeTransition();
    });
    Object.defineProperty(document, "startViewTransition", {
      value: startViewTransition,
      configurable: true,
    });
    document.documentElement.classList.remove("theme-vt");
    document.documentElement.style.removeProperty("--color-artwork-primary");
  });

  afterEach(() => {
    delete (document as { startViewTransition?: unknown }).startViewTransition;
  });

  it("applies the first theme directly, then crossfades later changes", () => {
    const store = new ThemeStore();
    store.gpuCompositing = true;
    store.applyActiveTheme();
    expect(startViewTransition).not.toHaveBeenCalled();
    expect(document.documentElement.classList.contains("theme-vt")).toBe(true);

    store.activeThemeId = "nordic-blue";
    store.applyActiveTheme();
    expect(startViewTransition).toHaveBeenCalledTimes(1);
    expect(document.documentElement.style.getPropertyValue("--glass-solid-sidebar")).toMatch(
      /^#[0-9a-f]{6}$/,
    );
  });

  it("writes Dynamic Artwork colors and the theme inside one transition", () => {
    const store = new ThemeStore();
    store.gpuCompositing = true;
    store.applyActiveTheme();
    store.activeThemeId = "dynamic-artwork";
    startViewTransition.mockImplementation(() => fakeTransition());

    store.applyArtworkColors({
      primary: "#112233",
      sidebar: "#223344",
      playerbar: "#334455",
      accent: "#445566",
      accentHover: "#556677",
      border: "#667788",
    });

    // Nothing written yet: the update runs inside the transition, after the
    // old frame is snapshotted.
    expect(startViewTransition).toHaveBeenCalledTimes(1);
    expect(document.documentElement.style.getPropertyValue("--color-artwork-primary")).not.toBe(
      "#112233",
    );

    const update = startViewTransition.mock.calls[0][0] as () => void;
    update();
    expect(document.documentElement.style.getPropertyValue("--color-artwork-primary")).toBe(
      "#112233",
    );
    expect(startViewTransition).toHaveBeenCalledTimes(1);
  });

  it("never uses a View Transition without confirmed GPU compositing (WebKitGTK crashes without it)", () => {
    const store = new ThemeStore();
    store.gpuCompositing = false;
    store.applyActiveTheme();
    store.activeThemeId = "nordic-blue";
    store.applyActiveTheme();
    expect(startViewTransition).not.toHaveBeenCalled();
    expect(document.documentElement.classList.contains("theme-vt")).toBe(false);
  });

  it("never uses a View Transition before the backend has answered", () => {
    const store = new ThemeStore();
    expect(store.gpuCompositing).toBeNull();
    store.applyActiveTheme();
    store.activeThemeId = "nordic-blue";
    store.applyActiveTheme();
    expect(startViewTransition).not.toHaveBeenCalled();
  });

  it("init() takes GPU compositing support from the backend", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "webview_gpu_compositing") return true;
      return {};
    });
    const store = new ThemeStore();
    await store.init();
    expect(store.gpuCompositing).toBe(true);
  });

  it("ends the crossfade at the first pointer input hit-tested to <html>, then stops listening", async () => {
    const store = new ThemeStore();
    store.gpuCompositing = true;
    store.applyActiveTheme();
    let finish!: () => void;
    const transition = {
      ready: new Promise<void>(() => {}),
      finished: new Promise<void>((resolve) => (finish = resolve)),
      skipTransition: vi.fn(),
    };
    startViewTransition.mockImplementation((update: () => void) => {
      update();
      return transition;
    });

    store.activeThemeId = "nordic-blue";
    store.applyActiveTheme();

    document.body.dispatchEvent(new Event("pointermove", { bubbles: true }));
    expect(transition.skipTransition).not.toHaveBeenCalled();

    document.documentElement.dispatchEvent(new Event("pointermove", { bubbles: true }));
    expect(transition.skipTransition).toHaveBeenCalledTimes(1);

    finish();
    await transition.finished;
    await new Promise((resolve) => setTimeout(resolve, 0));
    document.documentElement.dispatchEvent(new Event("wheel", { bubbles: true }));
    expect(transition.skipTransition).toHaveBeenCalledTimes(1);
  });

  it("treats a skipped crossfade as expected, not an unhandled rejection", async () => {
    const store = new ThemeStore();
    store.gpuCompositing = true;
    store.applyActiveTheme();
    const skipped = new DOMException("Transition was skipped", "AbortError");
    startViewTransition.mockImplementation((update: () => void) => {
      update();
      return {
        ready: Promise.reject(skipped),
        finished: Promise.resolve(),
        skipTransition: vi.fn(),
      };
    });
    const unhandled = vi.fn();
    process.on("unhandledRejection", unhandled);
    try {
      store.activeThemeId = "nordic-blue";
      store.applyActiveTheme();
      await new Promise((resolve) => setTimeout(resolve, 0));
      expect(unhandled).not.toHaveBeenCalled();
    } finally {
      process.off("unhandledRejection", unhandled);
    }
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

describe("custom themes saved with unreadable text", () => {
  it("resolves readable text for a custom theme whose stored text fails contrast", async () => {
    const store = new ThemeStore();
    store.customThemes = [
      {
        id: "custom-cream",
        name: "Cream",
        isCustom: true,
        colors: {
          ...LUMINOUS_DARK_COLORS,
          "bg-main": "#eee9df",
          "bg-sidebar": "#e5e0d4",
          "bg-playerbar": "#e5e0d4",
        },
      },
    ];
    store.activeThemeId = "custom-cream";
    expect(store.resolvedColors["color-text-primary"]).toBe(
      LUMINOUS_LIGHT_COLORS["color-text-primary"],
    );
  });
});

describe("resolveDockGlowColors (#2046)", () => {
  const lightnessOf = (hex: string) => {
    const { r, g, b } = hexToRgb(hex);
    return rgbToHsl(r, g, b).l;
  };

  it("uses artwork archetype palette when cover art is available and clamps lightness to dark theme bounds", () => {
    const artwork = {
      primary: "#111111",
      sidebar: "#111111",
      playerbar: "#111111",
      accent: "#ff0000",
      accentHover: "#ff3333",
      border: "#333333",
      vibrant: "#ff2222",
      darkVibrant: "#550000",
      lightVibrant: "#ffdddd",
      muted: "#886666",
    };

    const colors = resolveDockGlowColors(artwork, "#4f5bd5", true);
    expect(colors).toHaveLength(4);
    for (const hex of colors) {
      expect(lightnessOf(hex)).toBeLessThanOrEqual(DOCK_GLOW_DARK_MAX_L + 0.01);
    }
  });

  it("lifts dark artwork colors to the light-theme minimum lightness when a light theme is active", () => {
    const artwork = {
      primary: "#111111",
      sidebar: "#111111",
      playerbar: "#111111",
      accent: "#224488",
      accentHover: "#335599",
      border: "#333333",
      vibrant: "#112244",
      darkVibrant: "#050a14",
    };

    const colors = resolveDockGlowColors(artwork, "#4f5bd5", false);
    expect(colors).toHaveLength(2);
    for (const hex of colors) {
      expect(lightnessOf(hex)).toBeGreaterThanOrEqual(DOCK_GLOW_LIGHT_MIN_L - 0.01);
    }
  });

  it("falls back to the active theme accent when no artwork is present", () => {
    const colors = resolveDockGlowColors(null, "#4f5bd5", true);
    expect(colors).toHaveLength(1);
    expect(lightnessOf(colors[0])).toBeLessThanOrEqual(DOCK_GLOW_DARK_MAX_L + 0.01);
  });
});
