import { invoke } from "@tauri-apps/api/core";
import { getCoverArtUrl } from "../types";
import type { Song } from "../types";
import {
  hexToRgb,
  rgbToHex,
  pickAccessibleOnColor,
  clampForContrast,
  isLightColor,
  rgbToHsl,
  hslToRgb,
  quantizeMedianCut,
  extractArchetypes,
  checkWcagCompliance,
  generatePaletteFromSeed,
  type ColorCount
} from "../utils/colorUtils";

export interface ThemeColors {
  "bg-main": string;
  "bg-sidebar": string;
  "bg-playerbar": string;
  "color-accent": string;
  "color-accent-hover": string;
  "color-text-primary": string;
  "color-text-secondary": string;
  "color-border": string;
}

export interface Theme {
  id: string;
  name: string;
  colors: ThemeColors;
  isCustom?: boolean;
}

export interface ExtractedColors {
  primary: string;
  sidebar: string;
  playerbar: string;
  accent: string;
  accentHover: string;
  border: string;
}

/**
 * Reserved for ReactiveLogoBrand's gradient only (see calculateLogoStops()
 * below) — pulled from app-icon.svg's own sunrise gradient ("Vibrant Zest
 * Orange core" / "golden horizon" stops) so the in-app logo always matches
 * the real icon shown in the OS taskbar/dock, independent of whatever UI
 * accent the active theme uses. Not extracted programmatically from the
 * SVG at build/runtime — overkill for a static, rarely-changing asset, and
 * would still require guessing which of five gradient stops is "the" brand
 * color; using the exact hex the icon's own source comments already label
 * as the core color is simpler and unambiguous.
 */
const BRAND_ORANGE = "#ff7300";
const BRAND_GOLD = "#ffcc00";

/**
 * Dusty slate blue, chosen over the original BRAND_ORANGE for the dark
 * theme's UI accent (badges, buttons, sliders, active states) — clears the
 * same strict 4.5:1 WCAG text-contrast threshold against the near-black
 * canvas (~4.96:1) that BRAND_ORANGE cleared. Deliberately independent of
 * BRAND_ORANGE/BRAND_GOLD, which stay logo-only (see above) so this swap
 * doesn't touch the app-icon-matched logo gradient.
 */
const LUMINOUS_DARK_ACCENT = "#6f7ea9";

/**
 * "System" auto-theme: adapts to the OS light/dark preference. Panels
 * (sidebar, player bar, top nav) get a "glass" treatment via
 * backdrop-filter blur/saturate plus a tonal step from the canvas, applied
 * in app.css's .glass-surface class. Colors here stay fully opaque hex
 * (not rgba) on purpose — native <input type="color"> swatches in the
 * theme builder can't represent alpha, so translucent values would
 * silently break editing.
 */
export const LUMINOUS_DARK_COLORS: ThemeColors = {
  "bg-main": "#08090c",
  "bg-sidebar": "#1c1f29",
  "bg-playerbar": "#191b23",
  "color-accent": LUMINOUS_DARK_ACCENT,
  "color-accent-hover": blendToward(LUMINOUS_DARK_ACCENT, 255, 0.2),
  "color-text-primary": "#f1f3f8",
  "color-text-secondary": "#a6adc4",
  "color-border": "#2c2f3c"
};

/**
 * Same hex as LUMINOUS_DARK_ACCENT — unlike the old orange accent (which
 * had to darken into brown/rust to read against a light canvas), this slate
 * blue already clears WCAG 1.4.11's 3:1 non-text threshold as-is (~3.34:1
 * against this light canvas), so both schemes can share one literal color.
 */
const LUMINOUS_LIGHT_ACCENT = LUMINOUS_DARK_ACCENT;

export const LUMINOUS_LIGHT_COLORS: ThemeColors = {
  "bg-main": "#e9eaf0",
  "bg-sidebar": "#ffffff",
  "bg-playerbar": "#ffffff",
  "color-accent": LUMINOUS_LIGHT_ACCENT,
  "color-accent-hover": blendToward(LUMINOUS_LIGHT_ACCENT, 255, 0.2),
  "color-text-primary": "#16181d",
  "color-text-secondary": "#5a6072",
  "color-border": "#dcdce4"
};

/** Blends a hex color toward white (factor > 0) or black (factor < 0). */
export function blendToward(hex: string, target: 0 | 255, amount: number): string {
  const rgb = hexToRgb(hex);
  const mix = (c: number) => Math.round(c + (target - c) * amount);
  return rgbToHex(mix(rgb.r), mix(rgb.g), mix(rgb.b));
}

/**
 * Derives an rgba() string from an opaque hex color for glass-panel
 * rendering only. The "official" ThemeColors stay opaque hex everywhere
 * else (native <input type="color"> swatches, contrast tests, "Import
 * Active Colors") — alpha is applied here, one level removed, purely for
 * the .glass-surface CSS custom properties so it can never reach a color
 * picker's bound value.
 */
export function hexToRgbaString(hex: string, alpha: number): string {
  const { r, g, b } = hexToRgb(hex);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

/**
 * Ruby Red / Nordic Blue / Retro Amber are each defined as a single seed
 * hue (their accent color) run through generatePaletteFromSeed()
 * (colorUtils.ts) rather than 8 hand-picked hexes — "bake-once": computed
 * from the seed every time this module loads, not hand-tuned per value, so
 * every derived surface automatically keeps the WCAG guarantees
 * generatePaletteFromSeed() enforces (#61). The seed is just the theme's
 * previously hand-picked accent hex, chosen to preserve each theme's
 * identifying hue rather than introduce a new one.
 */
const RUBY_RED_SEED = "#e11d48";
const NORDIC_BLUE_SEED = "#88c0d0";
const RETRO_AMBER_SEED = "#d97706";

export const PREDEFINED_THEMES: Theme[] = [
  {
    id: "system",
    name: "System",
    colors: { ...LUMINOUS_DARK_COLORS }
  },
  {
    id: "ruby-red",
    name: "Ruby Red",
    colors: generatePaletteFromSeed(RUBY_RED_SEED)
  },
  {
    id: "nordic-blue",
    name: "Nordic Blue",
    colors: generatePaletteFromSeed(NORDIC_BLUE_SEED)
  },
  {
    id: "retro-amber",
    name: "Retro Amber",
    colors: generatePaletteFromSeed(RETRO_AMBER_SEED)
  },
  {
    id: "dynamic-artwork",
    name: "Dynamic Artwork",
    colors: {
      "bg-main": "var(--color-artwork-primary)",
      "bg-sidebar": "var(--color-artwork-sidebar)",
      "bg-playerbar": "var(--color-artwork-playerbar)",
      "color-accent": "var(--color-artwork-accent)",
      "color-accent-hover": "var(--color-artwork-accent-hover)",
      "color-text-primary": "#ffffff",
      "color-text-secondary": "#e2e8f0",
      "color-border": "var(--color-artwork-border)"
    }
  }
];

// The Dynamic Artwork theme (PREDEFINED_THEMES below) hardcodes these as its
// text colors regardless of the extracted background, so every background
// surface buildExtractedColors() produces must stay readable against both.
const ARTWORK_TEXT_PRIMARY = "#ffffff";
const ARTWORK_TEXT_SECONDARY = "#e2e8f0";

type Rgb = { r: number; g: number; b: number };

/** Re-lightens/darkens an RGB color in HSL space, holding hue+saturation fixed. */
function withLightness(rgb: Rgb, l: number): Rgb {
  const hsl = rgbToHsl(rgb.r, rgb.g, rgb.b);
  return hslToRgb(hsl.h, hsl.s, Math.min(1, Math.max(0, l)));
}

function clampLightness(rgb: Rgb, minL: number, maxL: number): Rgb {
  const hsl = rgbToHsl(rgb.r, rgb.g, rgb.b);
  if (hsl.l >= minL && hsl.l <= maxL) return rgb;
  return withLightness(rgb, Math.min(Math.max(hsl.l, minL), maxL));
}

/** Steps lightness down in HSL space until both fixed artwork text colors clear WCAG AA, or L bottoms out. */
function darkenUntilReadable(rgb: Rgb): Rgb {
  let candidate = rgb;
  let hsl = rgbToHsl(rgb.r, rgb.g, rgb.b);
  for (let i = 0; i < 30; i++) {
    const hex = rgbToHex(candidate.r, candidate.g, candidate.b);
    const primaryOk = checkWcagCompliance(ARTWORK_TEXT_PRIMARY, hex).wcagAA;
    const secondaryOk = checkWcagCompliance(ARTWORK_TEXT_SECONDARY, hex).wcagAA;
    if (primaryOk && secondaryOk) return candidate;
    if (hsl.l <= 0) return candidate;
    hsl = { ...hsl, l: Math.max(0, hsl.l - 0.02) };
    candidate = hslToRgb(hsl.h, hsl.s, hsl.l);
  }
  return candidate;
}

/**
 * Derives the full artwork palette from a weighted color histogram using
 * Median Cut quantization + Android Palette-style archetype scoring (#61),
 * replacing flat population-dominance picking. That approach loses small,
 * vibrant accent clusters on covers dominated by a huge neutral background
 * (e.g. a mostly-black album with a tiny neon accent) because population
 * alone decides the winner; scoring candidates by role (vibrant vs. muted,
 * light vs. dark) instead keeps the accent regardless of how few pixels it
 * covers. Every derived background surface is then validated with
 * checkWcagCompliance() against the fixed Dynamic Artwork text colors,
 * mirroring the pickAccessibleOnColor() pattern already used for text-on-accent.
 */
export function buildExtractedColors(colorCounts: ColorCount[]): ExtractedColors {
  const swatches = quantizeMedianCut(colorCounts, 24);
  const archetypes = extractArchetypes(swatches);
  const dominant = swatches.reduce((max, s) => (s.population > max.population ? s : max), swatches[0]);

  const primaryBase = archetypes.darkVibrant || archetypes.darkMuted || dominant;
  const primaryRgb = darkenUntilReadable(clampLightness(primaryBase, 0, 0.35));
  const primaryHsl = rgbToHsl(primaryRgb.r, primaryRgb.g, primaryRgb.b);

  // extractArchetypes() always backfills vibrant with the dominant swatch
  // when nothing clears its guard rails, so this is never null here.
  const accentBase = archetypes.vibrant as (typeof swatches)[number];
  const accentRgb = clampLightness(accentBase, 0.35, 0.75);
  const accentHsl = rgbToHsl(accentRgb.r, accentRgb.g, accentRgb.b);

  const sidebarRgb = withLightness(primaryRgb, primaryHsl.l - 0.025);
  const playerbarRgb = withLightness(primaryRgb, primaryHsl.l - 0.012);
  const borderRgb = withLightness(primaryRgb, primaryHsl.l + 0.14);
  const accentHoverRgb = withLightness(accentRgb, Math.min(0.85, accentHsl.l + 0.1));

  return {
    primary: rgbToHex(primaryRgb.r, primaryRgb.g, primaryRgb.b),
    sidebar: rgbToHex(sidebarRgb.r, sidebarRgb.g, sidebarRgb.b),
    playerbar: rgbToHex(playerbarRgb.r, playerbarRgb.g, playerbarRgb.b),
    accent: rgbToHex(accentRgb.r, accentRgb.g, accentRgb.b),
    accentHover: rgbToHex(accentHoverRgb.r, accentHoverRgb.g, accentHoverRgb.b),
    border: rgbToHex(borderRgb.r, borderRgb.g, borderRgb.b)
  };
}

export function extractColorsFromImage(imgUrl: string): Promise<ExtractedColors> {
  return new Promise((resolve) => {
    const img = new Image();
    img.crossOrigin = "Anonymous";
    img.onload = () => {
      try {
        const canvas = document.createElement("canvas");
        const ctx = canvas.getContext("2d");
        if (!ctx) {
          resolve(getFallbackColors());
          return;
        }

        canvas.width = 40;
        canvas.height = 40;
        ctx.drawImage(img, 0, 0, 40, 40);

        const imgData = ctx.getImageData(0, 0, 40, 40);
        const data = imgData.data;

        const colorBuckets = new Map<string, number>();
        let count = 0;

        for (let i = 0; i < data.length; i += 4) {
          const r = data[i];
          const g = data[i + 1];
          const b = data[i + 2];
          const a = data[i + 3];

          if (a < 200) continue; // skip transparent

          count++;

          // Pre-quantize to a 16-step RGB grid so Median Cut works over a
          // manageable candidate pool instead of up to 1600 raw pixels.
          const qr = Math.floor(r / 16) * 16;
          const qg = Math.floor(g / 16) * 16;
          const qb = Math.floor(b / 16) * 16;
          const key = `${qr},${qg},${qb}`;

          colorBuckets.set(key, (colorBuckets.get(key) || 0) + 1);
        }

        if (count === 0) {
          resolve(getFallbackColors());
          return;
        }

        const colorCounts: ColorCount[] = Array.from(colorBuckets.entries()).map(([key, bucketCount]) => {
          const [r, g, b] = key.split(",").map(Number);
          return { r, g, b, count: bucketCount };
        });

        resolve(buildExtractedColors(colorCounts));
      } catch (e) {
        console.error("Failed to process image colors:", e);
        resolve(getFallbackColors());
      }
    };

    img.onerror = () => {
      resolve(getFallbackColors());
    };

    img.src = imgUrl;
  });
}

function getFallbackColors(): ExtractedColors {
  return {
    primary: "#2e3440",
    sidebar: "#242933",
    playerbar: "#2b303c",
    accent: "#88c0d0",
    accentHover: "#8fbcbb",
    border: "#3b4252"
  };
}

function calculateLogoStops(accentHex: string, accentHoverHex: string) {
  const darken = (hex: string, amount: number): string => {
    if (!hex || !hex.startsWith("#")) return hex || "";
    const usePound = hex[0] === "#";
    const col = usePound ? hex.slice(1) : hex;
    const num = parseInt(col, 16);
    const r = Math.max(0, Math.floor(((num / 65536) % 256) * (1 - amount)));
    const g = Math.max(0, Math.floor(((num / 256) % 256) * (1 - amount)));
    const b = Math.max(0, Math.floor((num % 256) * (1 - amount)));
    return (usePound ? "#" : "") + (0x1000000 + r * 0x10000 + g * 0x100 + b).toString(16).slice(1);
  };

  return {
    stop1: darken(accentHex, 0.6),
    stop2: darken(accentHex, 0.2),
    stop3: accentHex,
    stop4: accentHoverHex
  };
}

class ThemeStore {
  activeThemeId = $state<string>("system");
  customThemes = $state<Theme[]>([]);
  artworkColors = $state<ExtractedColors | null>(null);
  systemColorScheme = $state<"light" | "dark">("dark");

  constructor() {}

  async init() {
    this.watchSystemColorScheme();

    try {
      const settings = await invoke<Record<string, string>>("get_all_app_settings");
      if (settings) {
        if (settings.custom_themes) {
          try {
            this.customThemes = JSON.parse(settings.custom_themes);
          } catch (e) {
            console.error("Failed to parse custom_themes:", e);
          }
        }
        if (settings.active_theme_id) {
          const themeId = settings.active_theme_id;
          if (PREDEFINED_THEMES.some(t => t.id === themeId) || this.customThemes.some(t => t.id === themeId)) {
            this.activeThemeId = themeId;
          }
        }
      }
      this.applyActiveTheme();
    } catch (e) {
      console.error("Failed to init ThemeStore:", e);
      this.applyActiveTheme();
    }
  }

  /**
   * Reads the OS light/dark preference and listens for changes so the
   * "Luminous" auto-theme (and its logo gradient, computed in JS from a
   * literal hex accent) can react live without a page reload.
   */
  watchSystemColorScheme() {
    if (typeof window === "undefined" || !window.matchMedia) return;
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    this.systemColorScheme = mq.matches ? "dark" : "light";
    mq.addEventListener("change", (e) => {
      this.systemColorScheme = e.matches ? "dark" : "light";
      if (this.activeThemeId === "system") {
        this.applyActiveTheme();
      }
    });
  }

  get isGlassTheme(): boolean {
    // Every theme gets the glass treatment now — chrome panels always
    // render translucent/blurred, computed from whichever theme is active.
    return true;
  }

  get currentTheme(): Theme {
    const predefined = PREDEFINED_THEMES.find(t => t.id === this.activeThemeId);
    if (predefined) return predefined;
    const custom = this.customThemes.find(t => t.id === this.activeThemeId);
    return custom || PREDEFINED_THEMES.find(t => t.id === "system") || PREDEFINED_THEMES[0];
  }

  /**
   * The active theme's actual literal hex colors — resolves System's
   * scheme-dependent palette and Dynamic Artwork's `var(--color-artwork-*)`
   * references to real values. Use this (not currentTheme.colors, and
   * never getComputedStyle of the live CSS custom properties) whenever a
   * UI component needs the theme's true colors: reading the live CSS vars
   * is unreliable while Design Tools' live-preview is active, since that
   * preview temporarily overwrites those same custom properties with
   * whatever's being edited.
   */
  get resolvedColors(): ThemeColors {
    const theme = this.currentTheme;
    if (theme.id === "system") {
      return this.systemColorScheme === "dark" ? LUMINOUS_DARK_COLORS : LUMINOUS_LIGHT_COLORS;
    }
    if (theme.id === "dynamic-artwork") {
      const artColors = this.artworkColors || getFallbackColors();
      return {
        ...theme.colors,
        "bg-main": artColors.primary,
        "bg-sidebar": artColors.sidebar,
        "bg-playerbar": artColors.playerbar,
        "color-accent": artColors.accent,
        "color-accent-hover": artColors.accentHover,
        "color-border": artColors.border
      };
    }
    return theme.colors;
  }

  async setTheme(themeId: string) {
    if (PREDEFINED_THEMES.some(t => t.id === themeId) || this.customThemes.some(t => t.id === themeId)) {
      this.activeThemeId = themeId;
      this.applyActiveTheme();
      await invoke("set_app_setting", { key: "active_theme_id", value: themeId }).catch(err => {
        console.error("Failed to save active_theme_id:", err);
      });
    }
  }

  async addCustomTheme(theme: Theme) {
    const existingIndex = this.customThemes.findIndex(t => t.id === theme.id);
    if (existingIndex >= 0) {
      this.customThemes[existingIndex] = theme;
    } else {
      this.customThemes.push(theme);
    }
    this.activeThemeId = theme.id;
    this.applyActiveTheme();

    await invoke("set_app_setting", { key: "custom_themes", value: JSON.stringify(this.customThemes) }).catch(err => {
      console.error("Failed to save custom_themes:", err);
    });
    await invoke("set_app_setting", { key: "active_theme_id", value: theme.id }).catch(err => {
      console.error("Failed to save active_theme_id:", err);
    });
  }

  async deleteCustomTheme(themeId: string) {
    this.customThemes = this.customThemes.filter(t => t.id !== themeId);
    await invoke("set_app_setting", { key: "custom_themes", value: JSON.stringify(this.customThemes) }).catch(err => {
      console.error("Failed to save custom_themes:", err);
    });

    if (this.activeThemeId === themeId) {
      await this.setTheme("system");
    }
  }

  async updateArtworkColors(song: Song | undefined) {
    // Extracted artwork colors only drive rendering (--logo-stop-*,
    // --color-artwork-*) when the Dynamic Artwork theme is actually
    // active — otherwise every track change would stomp whatever
    // applyActiveTheme() correctly set for the current theme. The
    // extraction itself still runs so artworkColors is ready the moment
    // the user switches to Dynamic Artwork.
    const isDynamicArtwork = this.currentTheme.id === "dynamic-artwork";

    if (!song) {
      if (isDynamicArtwork) this.resetArtworkColors();
      else this.artworkColors = null;
      return;
    }

    let url: string | null = null;
    if (song.art_manual) {
      if (song.art_manual.startsWith("http://") || song.art_manual.startsWith("https://") || song.art_manual.startsWith("/")) {
        url = song.art_manual;
      } else {
        url = getCoverArtUrl(`luminous-art://${song.art_manual}`);
      }
    } else if (song.art_automatic) {
      if (song.art_automatic.startsWith("http://") || song.art_automatic.startsWith("https://") || song.art_automatic.startsWith("/")) {
        url = song.art_automatic;
      } else if (song.art_automatic.startsWith("album-")) {
        url = getCoverArtUrl(`luminous-art://${song.art_automatic}`);
      } else {
        url = getCoverArtUrl(`luminous-art://local/${song.art_automatic}`);
      }
    } else if (song.art_embedded) {
      try {
        const uri = await invoke<string | null>("get_cover_art_uri", { songId: song.id });
        if (uri) {
          url = getCoverArtUrl(uri);
        }
      } catch (e) {
        console.error("Failed to query cover art URI in themeStore:", e);
      }
    }

    if (!url) {
      if (isDynamicArtwork) this.resetArtworkColors();
      else this.artworkColors = null;
      return;
    }

    try {
      const colors = await extractColorsFromImage(url);
      if (isDynamicArtwork) this.applyArtworkColors(colors);
      else this.artworkColors = colors;
    } catch (e) {
      console.error("Failed to extract artwork colors:", e);
      if (isDynamicArtwork) this.resetArtworkColors();
      else this.artworkColors = null;
    }
  }

  applyArtworkColors(colors: ExtractedColors) {
    this.artworkColors = colors;
    if (typeof document === "undefined") return;
    const root = document.documentElement;
    root.style.setProperty("--color-artwork-primary", colors.primary);
    root.style.setProperty("--color-artwork-sidebar", colors.sidebar);
    root.style.setProperty("--color-artwork-playerbar", colors.playerbar);
    root.style.setProperty("--color-artwork-accent", colors.accent);
    root.style.setProperty("--color-artwork-accent-hover", colors.accentHover);
    root.style.setProperty("--color-artwork-border", colors.border);

    // Apply logo stop variables
    const stops = calculateLogoStops(colors.accent, colors.accentHover);
    root.style.setProperty("--logo-stop-1", stops.stop1);
    root.style.setProperty("--logo-stop-2", stops.stop2);
    root.style.setProperty("--logo-stop-3", stops.stop3);
    root.style.setProperty("--logo-stop-4", stops.stop4);
  }

  resetArtworkColors() {
    this.artworkColors = null;
    if (typeof document === "undefined") return;
    const root = document.documentElement;
    root.style.setProperty("--color-artwork-primary", "#2e3440");
    root.style.setProperty("--color-artwork-sidebar", "#242933");
    root.style.setProperty("--color-artwork-playerbar", "#2b303c");
    root.style.setProperty("--color-artwork-accent", "#88c0d0");
    root.style.setProperty("--color-artwork-accent-hover", "#8fbcbb");
    root.style.setProperty("--color-artwork-border", "#3b4252");

    // Apply logo stop variables
    const stops = calculateLogoStops("#88c0d0", "#8fbcbb");
    root.style.setProperty("--logo-stop-1", stops.stop1);
    root.style.setProperty("--logo-stop-2", stops.stop2);
    root.style.setProperty("--logo-stop-3", stops.stop3);
    root.style.setProperty("--logo-stop-4", stops.stop4);
  }

  applyActiveTheme() {
    if (typeof document === "undefined") return;
    const theme = this.currentTheme;
    const isLuminous = theme.id === "system";
    // The System theme's live colors come from whichever OS-scheme palette
    // is active, not the static preview colors on the theme entry.
    const colors = isLuminous
      ? (this.systemColorScheme === "dark" ? LUMINOUS_DARK_COLORS : LUMINOUS_LIGHT_COLORS)
      : theme.colors;

    // Heuristically derived, not hand-picked: text rendered directly on
    // the accent color (active nav items, filled buttons) needs contrast
    // against whatever that accent happens to be — including a
    // user-chosen custom-theme accent — not just the canvas-tuned
    // text-primary/secondary. Computed for every theme, always kept in
    // sync with the active accent. Dynamic Artwork's `color-accent` is a
    // CSS var reference (not a literal hex), so resolve it to the real
    // extracted color first.
    const resolvedAccent = theme.id === "dynamic-artwork"
      ? (this.artworkColors || getFallbackColors()).accent
      : colors["color-accent"];
    const accentContrastText = pickAccessibleOnColor(resolvedAccent);

    // "Accent Text" — the only accent-derived color allowed on text/icons
    // (everything else must be Primary or Secondary text). Unlike
    // resolvedAccent (used for solid-fill surfaces, where accentContrastText
    // above supplies the on-top text color), this is the accent itself
    // clamped to WCAG AA 4.5:1 against bg-main, since raw accent-as-text has
    // no such guarantee for custom or Dynamic Artwork themes, and even
    // hand-picked theme accents are only checked against bg-main directly —
    // not the translucent accent-tinted surfaces (badges, hover states) text
    // often actually renders on. Both bg-main and accent-hover need the same
    // dynamic-artwork CSS-var-reference resolution as resolvedAccent above.
    const resolvedBgMain = theme.id === "dynamic-artwork"
      ? (this.artworkColors || getFallbackColors()).primary
      : colors["bg-main"];
    const resolvedAccentHover = theme.id === "dynamic-artwork"
      ? (this.artworkColors || getFallbackColors()).accentHover
      : colors["color-accent-hover"];
    const accentText = clampForContrast(resolvedAccent, resolvedBgMain, 4.5);
    const accentTextHover = clampForContrast(resolvedAccentHover, resolvedBgMain, 4.5);

    let styleEl = document.getElementById("luminous-theme-style");
    if (!styleEl) {
      styleEl = document.createElement("style");
      styleEl.id = "luminous-theme-style";
      document.head.appendChild(styleEl);
    }

    styleEl.innerHTML = `
      :root {
        --bg-main: ${colors["bg-main"]};
        --bg-sidebar: ${colors["bg-sidebar"]};
        --bg-playerbar: ${colors["bg-playerbar"]};
        --color-accent: ${colors["color-accent"]};
        --color-accent-hover: ${colors["color-accent-hover"]};
        --color-text-primary: ${colors["color-text-primary"]};
        --color-text-secondary: ${colors["color-text-secondary"]};
        --color-border: ${colors["color-border"]};
        --color-accent-contrast: ${accentContrastText};
        --color-accent-text: ${accentText};
        --color-accent-text-hover: ${accentTextHover};
      }
    `;

    const root = document.documentElement;
    root.classList.toggle("theme-glass", true);

    // Glass rendering vars — computed for every theme (not just System) so
    // all four chrome panels get the blur/tint/shine treatment regardless
    // of which theme is active. isDark comes from this theme's own
    // bg-main luminance rather than systemColorScheme, since only System
    // tracks the OS scheme — every other theme has fixed colors.
    // Rendering-only, separate from the opaque `colors` above — alpha
    // never reaches a color picker, see hexToRgbaString().
    const isDark = !isLightColor(colors["bg-main"]);
    root.style.setProperty("--glass-bg-sidebar", hexToRgbaString(colors["bg-sidebar"], isDark ? 0.5 : 0.6));
    root.style.setProperty("--glass-bg-playerbar", hexToRgbaString(colors["bg-playerbar"], isDark ? 0.78 : 0.85));
    root.style.setProperty("--glass-border-color", isDark ? "rgba(255, 255, 255, 0.10)" : "rgba(15, 15, 20, 0.08)");

    const elevation = isDark ? "0 8px 32px rgba(0, 0, 0, 0.45)" : "0 8px 32px rgba(15, 15, 20, 0.10)";
    const highlight = isDark ? "inset 0 1px 0 rgba(255, 255, 255, 0.14)" : "inset 0 1px 0 rgba(255, 255, 255, 0.9)";
    root.style.setProperty("--glass-shadow", `${elevation}, ${highlight}`);

    // PlayDock-only accent glow — kept out of --glass-shadow above since
    // the other three panels don't get it. Two-layer glow (tight bright
    // core + wide soft halo) reads as an actual glow rather than a flat
    // blurred outline. Reuses resolvedAccent (computed above), not
    // colors["color-accent"] directly: for Dynamic Artwork that's a CSS
    // var() reference string, and hexToRgbaString() needs a literal hex —
    // fed the reference string, hexToRgb()'s regex fails and silently
    // falls back to black, rendering as an invisible glow.
    const glowNear = `0 0 24px 2px ${hexToRgbaString(resolvedAccent, isDark ? 0.45 : 0.28)}`;
    const glowFar = `0 0 90px 10px ${hexToRgbaString(resolvedAccent, isDark ? 0.28 : 0.16)}`;
    root.style.setProperty("--glass-glow", `${glowNear}, ${glowFar}`);

    // Apply logo stops based on active theme or dynamic colors. The
    // System theme always uses the true brand orange/gold here — never
    // the scheme-adjusted UI accent, which in light mode is deliberately
    // darkened for text contrast and would make the logo look muddy/wrong
    // instead of matching the real app icon.
    if (theme.id === "dynamic-artwork") {
      const artColors = this.artworkColors || getFallbackColors();
      const stops = calculateLogoStops(artColors.accent, artColors.accentHover);
      root.style.setProperty("--logo-stop-1", stops.stop1);
      root.style.setProperty("--logo-stop-2", stops.stop2);
      root.style.setProperty("--logo-stop-3", stops.stop3);
      root.style.setProperty("--logo-stop-4", stops.stop4);
    } else if (isLuminous) {
      const stops = calculateLogoStops(BRAND_ORANGE, BRAND_GOLD);
      root.style.setProperty("--logo-stop-1", stops.stop1);
      root.style.setProperty("--logo-stop-2", stops.stop2);
      root.style.setProperty("--logo-stop-3", stops.stop3);
      root.style.setProperty("--logo-stop-4", stops.stop4);
    } else {
      const stops = calculateLogoStops(colors["color-accent"], colors["color-accent-hover"]);
      root.style.setProperty("--logo-stop-1", stops.stop1);
      root.style.setProperty("--logo-stop-2", stops.stop2);
      root.style.setProperty("--logo-stop-3", stops.stop3);
      root.style.setProperty("--logo-stop-4", stops.stop4);
    }
  }
}

export const themeStore = new ThemeStore();
