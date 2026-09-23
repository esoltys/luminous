import { invoke } from "@tauri-apps/api/core";
import { getCoverArtUrl, resolveArtUrl } from "../types";
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
import { LIGHTNESS_STEP } from "../constants";
import { isLinux } from "../platform";

const MAX_READABILITY_ADJUST_STEPS = 30;

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
  vibrant?: string;
  lightVibrant?: string;
  darkVibrant?: string;
  muted?: string;
  lightMuted?: string;
  darkMuted?: string;
  isLight?: boolean;
  primary: string;
  sidebar: string;
  playerbar: string;
  accent: string;
  accentHover: string;
  border: string;
}

/**
 * Dusty slate blue, the dark theme's UI accent (badges, buttons, sliders,
 * active states) — clears the strict 4.5:1 WCAG text-contrast threshold
 * against the near-black canvas (~4.96:1). Per the Luminous Logo System
 * (docs/Luminous Logo System.dc.html), the in-app reactive logo's glow/ring
 * re-target to this same active-theme accent/accent-hover pair — the fixed
 * Indigo/Gold brand colors in app-icon.svg are reserved for the static
 * "at rest" mark (app icon, marketing) only.
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
  "color-border": "#3d4255"
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
  // #dcdce4 measured ~1.8:1 against the white sidebar/card surfaces here —
  // well under WCAG 1.4.11's 3:1 non-text contrast floor, so borders (e.g.
  // the playlist toolbar's icon buttons) were nearly invisible. This slate,
  // hue-matched to LUMINOUS_LIGHT_ACCENT, clears ~3.7:1.
  "color-border": "#7f84a0"
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

/** Glass panel tint alpha: dark themes 0.5, light themes 0.6. */
function glassAlpha(isDark: boolean): number {
  return isDark ? 0.5 : 0.6;
}

/**
 * Opaque equivalent of a .glass-surface panel whose only backdrop is the
 * flat bg-main canvas (sidebar, top nav, right panel — none of them ever
 * overlap other content): `blur(20px)` of a flat color is that same color,
 * so the panel's visible result is exactly the translucent tint composited
 * over `saturate(180%)` of bg-main. Painting that as a solid color keeps
 * the look while dropping backdrop-filter from those panels, which was
 * re-rasterized whenever anything nearby repainted (main-view hover
 * effects, the reactive logo, the playbar spectrum) and showed up as
 * banding/pulsing in the sidebar.
 */
export function flatGlassColor(tintHex: string, alpha: number, backdropHex: string, saturation = 1.8): string {
  const { r, g, b } = hexToRgb(backdropHex);
  const s = saturation;
  // Filter Effects `saturate()` matrix, applied in sRGB like the browser's
  // CSS filter shorthand.
  const sr = (0.213 + 0.787 * s) * r + (0.715 - 0.715 * s) * g + (0.072 - 0.072 * s) * b;
  const sg = (0.213 - 0.213 * s) * r + (0.715 + 0.285 * s) * g + (0.072 - 0.072 * s) * b;
  const sb = (0.213 - 0.213 * s) * r + (0.715 - 0.715 * s) * g + (0.072 + 0.928 * s) * b;
  const tint = hexToRgb(tintHex);
  const over = (t: number, back: number) =>
    Math.round(Math.min(255, Math.max(0, t * alpha + Math.min(255, Math.max(0, back)) * (1 - alpha))));
  return rgbToHex(over(tint.r, sr), over(tint.g, sg), over(tint.b, sb));
}

/**
 * Whether theme changes can crossfade as a View Transition. Never on Linux:
 * WebKitGTK exposes startViewTransition(), but with the DMA-BUF renderer
 * disabled — which the app always does there (LINUX_WEBKITGTK_RENDERING_ENV_VARS
 * in lib.rs) — the first transition segfaults the whole app (reproduced on
 * WebKitGTK 2.52.6). Linux keeps the @property morph instead.
 */
function supportsViewTransitions(): boolean {
  return !isLinux && typeof document !== "undefined" && typeof document.startViewTransition === "function";
}

const VIEW_TRANSITION_INPUT_EVENTS = ["pointermove", "pointerdown", "wheel"] as const;

/**
 * Ends a View Transition at the first pointer input that's hit-tested to
 * <html> rather than the live page. `pointer-events: none` on
 * ::view-transition (app.css) is meant to pass input through, but WebKit
 * ignores it and captures all input for the length of the transition, and
 * that hasn't been confirmed in WebView2 either — a 1.2s crossfade would otherwise
 * swallow hovers, clicks and wheel-scrolls on every track change under
 * Dynamic Artwork. Hit-testing is restored synchronously once skipped, so
 * the pointer movement that precedes a click lets that click land.
 */
function yieldViewTransitionToInput(transition: ViewTransition | undefined) {
  if (!transition) return;
  const root = document.documentElement;
  const onInput = (event: Event) => {
    if (event.target === root) transition.skipTransition();
  };
  for (const type of VIEW_TRANSITION_INPUT_EVENTS) {
    window.addEventListener(type, onInput, { capture: true, passive: true });
  }
  void transition.finished
    .catch(() => {})
    .finally(() => {
      for (const type of VIEW_TRANSITION_INPUT_EVENTS) {
        window.removeEventListener(type, onInput, { capture: true });
      }
    });
}

function prefersReducedMotion(): boolean {
  return typeof window !== "undefined" && !!window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
}

const RUBY_RED_COLORS: ThemeColors = {
  "bg-main": "#17110e",
  "bg-sidebar": "#6e0b1b",
  "bg-playerbar": "#17110e",
  "color-accent": "#b51021",
  "color-accent-hover": "#f06090",
  "color-text-primary": "#ffffff",
  "color-text-secondary": "#e2e8f0",
  "color-border": "#974a5f"
};

const RUBY_RED_SEED = "#e11d48";
const NORDIC_BLUE_SEED = "#88c0d0";
const RETRO_AMBER_SEED = "#d97706";

const NORDIC_BLUE_COLORS: ThemeColors = {
  "bg-main": "#2e515f",
  "bg-sidebar": "#000910",
  "bg-playerbar": "#2e515f",
  "color-accent": "#3090a0",
  "color-accent-hover": "#a3cfdc",
  "color-text-primary": "#ffffff",
  "color-text-secondary": "#e2e8f0",
  "color-border": "#55879a",
};

const METALLIC_COLORS: ThemeColors = {
  "bg-main": "#101d1d",
  "bg-sidebar": "#00020b",
  "bg-playerbar": "#101d1d",
  "color-accent": "#a18a47",
  "color-accent-hover": "#b7a267",
  "color-text-primary": "#ffffff",
  "color-text-secondary": "#e2e8f0",
  "color-border": "#af9350"
};

const SABRINA_COLORS: ThemeColors = {
  "bg-main": "#3E4B68",
  "bg-sidebar": "#280C00",
  "bg-playerbar": "#3D4A66",
  "color-accent": "#255098",
  "color-accent-hover": "#E9B787",
  "color-text-primary": "#ffffff",
  "color-text-secondary": "#e2e8f0",
  "color-border": "#85674C"
};

const TERMINAL_GREEN_COLORS: ThemeColors = {
  "bg-main": "#0f1f1d",
  "bg-sidebar": "#091514",
  "bg-playerbar": "#152a27",
  "color-accent": "#10b981",
  "color-accent-hover": "#34d399",
  "color-text-primary": "#f0fdfa",
  "color-text-secondary": "#99f6e4",
  "color-border": "#1e3a35"
};

/**
 * color-text-secondary is darkened slightly from the source palette's
 * #5a6072 (L 0.118) to #565c6e (L 0.108) — the original fell just under
 * WCAG AA (4.37:1) against bg-main/bg-playerbar's #ddd6d4 (4.5:1 required);
 * this is the minimal HSL-lightness nudge needed to clear it.
 */
const DRIFTWOOD_COLORS: ThemeColors = {
  "bg-main": "#ddd6d4",
  "bg-sidebar": "#dfdfdf",
  "bg-playerbar": "#ddd6d4",
  "color-accent": "#a6a6a6",
  "color-accent-hover": "#62361e",
  "color-text-primary": "#16181d",
  "color-text-secondary": "#565c6e",
  "color-border": "#997766"
};

const FOXGLOVE_COLORS: ThemeColors = {
  "bg-main": "#4a3948",
  "bg-sidebar": "#602336",
  "bg-playerbar": "#4a3948",
  "color-accent": "#654e62",
  "color-accent-hover": "#b4b7db",
  "color-text-primary": "#ffffff",
  "color-text-secondary": "#e2e8f0",
  "color-border": "#783a46"
};

export const PREDEFINED_THEMES: Theme[] = [
  {
    id: "dynamic-artwork",
    name: "✨ Luminous",
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
  },
  {
    id: "system",
    name: "System",
    colors: { ...LUMINOUS_DARK_COLORS }
  },
  {
    id: "ruby-red",
    name: "Ruby Red",
    colors: { ...RUBY_RED_COLORS }
  },
  {
    id: "nordic-blue",
    name: "Nordic Blue",
    colors: { ...NORDIC_BLUE_COLORS }
  },
  {
    id: "retro-amber",
    name: "Retro Amber",
    colors: generatePaletteFromSeed(RETRO_AMBER_SEED)
  },
  {
    id: "metallic",
    name: "Metallic",
    colors: { ...METALLIC_COLORS }
  },
  {
    id: "sabrina",
    name: "Sabrina",
    colors: { ...SABRINA_COLORS }
  },
  {
    id: "terminal-green",
    name: "Terminal Green",
    colors: { ...TERMINAL_GREEN_COLORS }
  },
  {
    id: "driftwood",
    name: "Driftwood",
    colors: { ...DRIFTWOOD_COLORS }
  },
  {
    id: "foxglove",
    name: "Foxglove",
    colors: { ...FOXGLOVE_COLORS }
  }
];

const ARTWORK_TEXT_PRIMARY_DARK = "#ffffff";
const ARTWORK_TEXT_SECONDARY_DARK = "#e2e8f0";
const ARTWORK_TEXT_PRIMARY_LIGHT = "#16181d";
const ARTWORK_TEXT_SECONDARY_LIGHT = "#5a6072";

/**
 * Single source of truth for Dynamic Artwork's text color, conditioned on
 * the extracted cover art's isLight. Shared by resolvedColors (used to seed
 * "Save as Custom Theme") and applyActiveTheme (the live-rendered CSS
 * vars) so the two can't drift apart again (#156).
 */
function getArtworkTextColors(artColors: ExtractedColors): Pick<ThemeColors, "color-text-primary" | "color-text-secondary"> {
  return artColors.isLight
    ? { "color-text-primary": ARTWORK_TEXT_PRIMARY_LIGHT, "color-text-secondary": ARTWORK_TEXT_SECONDARY_LIGHT }
    : { "color-text-primary": ARTWORK_TEXT_PRIMARY_DARK, "color-text-secondary": ARTWORK_TEXT_SECONDARY_DARK };
}

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

/** Steps lightness down in HSL space until both fixed dark-theme text colors clear WCAG AA, or L bottoms out. */
function darkenUntilReadable(rgb: Rgb): Rgb {
  let candidate = rgb;
  let hsl = rgbToHsl(rgb.r, rgb.g, rgb.b);
  for (let i = 0; i < MAX_READABILITY_ADJUST_STEPS; i++) {
    const hex = rgbToHex(candidate.r, candidate.g, candidate.b);
    const primaryOk = checkWcagCompliance(ARTWORK_TEXT_PRIMARY_DARK, hex).wcagAA;
    const secondaryOk = checkWcagCompliance(ARTWORK_TEXT_SECONDARY_DARK, hex).wcagAA;
    if (primaryOk && secondaryOk) return candidate;
    if (hsl.l <= 0) return candidate;
    hsl = { ...hsl, l: Math.max(0, hsl.l - LIGHTNESS_STEP) };
    candidate = hslToRgb(hsl.h, hsl.s, hsl.l);
  }
  return candidate;
}

/** Steps lightness up in HSL space until both fixed light-theme text colors clear WCAG AA, or L tops out. */
function lightenUntilReadable(rgb: Rgb): Rgb {
  let candidate = rgb;
  let hsl = rgbToHsl(rgb.r, rgb.g, rgb.b);
  for (let i = 0; i < MAX_READABILITY_ADJUST_STEPS; i++) {
    const hex = rgbToHex(candidate.r, candidate.g, candidate.b);
    const primaryOk = checkWcagCompliance(ARTWORK_TEXT_PRIMARY_LIGHT, hex).wcagAA;
    const secondaryOk = checkWcagCompliance(ARTWORK_TEXT_SECONDARY_LIGHT, hex).wcagAA;
    if (primaryOk && secondaryOk) return candidate;
    if (hsl.l >= 1) return candidate;
    hsl = { ...hsl, l: Math.min(1, hsl.l + LIGHTNESS_STEP) };
    candidate = hslToRgb(hsl.h, hsl.s, hsl.l);
  }
  return candidate;
}

export function buildExtractedColors(colorCounts: ColorCount[]): ExtractedColors {
  const swatches = quantizeMedianCut(colorCounts, 24);
  const archetypes = extractArchetypes(swatches);
  const dominant = swatches.reduce((max, s) => (s.population > max.population ? s : max), swatches[0]);

  // Derive all 6 Android Palette archetypes with fallbacks
  const vibrant = archetypes.vibrant || dominant;
  const lightVibrant = archetypes.lightVibrant || clampLightness(vibrant, 0.55, 0.85);
  const darkVibrant = archetypes.darkVibrant || clampLightness(vibrant, 0.15, 0.35);
  const muted = archetypes.muted || dominant;
  const lightMuted = archetypes.lightMuted || clampLightness(muted, 0.55, 0.85);
  const darkMuted = archetypes.darkMuted || clampLightness(muted, 0.15, 0.35);

  // Determine if cover art is light ($L > 0.40$)
  const dominantHsl = rgbToHsl(dominant.r, dominant.g, dominant.b);
  const isLight = dominantHsl.l > 0.40;

  // Map 6 archetypes to UI panel surfaces based on Light vs Dark Glass Mode
  let primaryRgb: Rgb;
  let sidebarRgb: Rgb;
  let playerbarRgb: Rgb;
  let accentRgb: Rgb;
  let accentHoverRgb: Rgb;
  let borderRgb: Rgb;

  if (isLight) {
    primaryRgb = lightenUntilReadable(clampLightness(lightMuted, 0.70, 0.95));
    sidebarRgb = lightenUntilReadable(clampLightness(lightVibrant, 0.65, 0.90));
    playerbarRgb = lightenUntilReadable(clampLightness(lightMuted, 0.68, 0.92));
    accentRgb = clampLightness(vibrant, 0.35, 0.65);
    accentHoverRgb = clampLightness(darkVibrant, 0.25, 0.55);
    borderRgb = clampLightness(muted, 0.50, 0.80);
  } else {
    primaryRgb = darkenUntilReadable(clampLightness(darkMuted, 0.0, 0.35));
    sidebarRgb = darkenUntilReadable(clampLightness(darkVibrant, 0.0, 0.30));
    playerbarRgb = darkenUntilReadable(clampLightness(darkMuted, 0.0, 0.32));
    accentRgb = clampLightness(vibrant, 0.35, 0.75);
    accentHoverRgb = clampLightness(lightVibrant, 0.45, 0.85);
    borderRgb = clampLightness(muted, 0.20, 0.50);
  }

  return {
    vibrant: rgbToHex(vibrant.r, vibrant.g, vibrant.b),
    lightVibrant: rgbToHex(lightVibrant.r, lightVibrant.g, lightVibrant.b),
    darkVibrant: rgbToHex(darkVibrant.r, darkVibrant.g, darkVibrant.b),
    muted: rgbToHex(muted.r, muted.g, muted.b),
    lightMuted: rgbToHex(lightMuted.r, lightMuted.g, lightMuted.b),
    darkMuted: rgbToHex(darkMuted.r, darkMuted.g, darkMuted.b),
    isLight,
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
    vibrant: "#88c0d0",
    lightVibrant: "#8fbcbb",
    darkVibrant: "#5e81ac",
    muted: "#4c566a",
    lightMuted: "#d8dee9",
    darkMuted: "#2e3440",
    isLight: false,
    primary: "#2e3440",
    sidebar: "#242933",
    playerbar: "#2b303c",
    accent: "#88c0d0",
    accentHover: "#8fbcbb",
    border: "#3b4252"
  };
}

export class ThemeStore {
  activeThemeId = $state<string>("system");
  customThemes = $state<Theme[]>([]);
  artworkColors = $state<ExtractedColors | null>(null);
  systemColorScheme = $state<"light" | "dark">("dark");
  /**
   * Sub-setting of the System theme (#692): "system" keeps following the OS
   * preference (default), while "light"/"dark" pin the System theme's
   * resolved scheme regardless of what the OS reports. Distinct from
   * systemColorScheme, which always tracks the raw OS preference so it's
   * still available to resolve against once the user switches back to
   * "system".
   */
  colorSchemeMode = $state<"light" | "dark" | "system">("system");

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
        if (settings.color_scheme_mode === "light" || settings.color_scheme_mode === "dark" || settings.color_scheme_mode === "system") {
          this.colorSchemeMode = settings.color_scheme_mode;
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
      if (this.activeThemeId === "system" && this.colorSchemeMode === "system") {
        this.applyActiveTheme();
      }
    });
  }

  /**
   * The System theme's actual resolved scheme: the OS preference when
   * colorSchemeMode is "system", or the pinned mode otherwise.
   */
  get effectiveColorScheme(): "light" | "dark" {
    return this.colorSchemeMode === "system" ? this.systemColorScheme : this.colorSchemeMode;
  }

  /** Pins or unpins the System theme's Light/Dark resolution (#692). */
  async setColorSchemeMode(mode: "light" | "dark" | "system") {
    this.colorSchemeMode = mode;
    if (this.activeThemeId === "system") {
      this.applyActiveTheme();
    }
    await invoke("set_app_setting", { key: "color_scheme_mode", value: mode });
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
      return this.effectiveColorScheme === "dark" ? LUMINOUS_DARK_COLORS : LUMINOUS_LIGHT_COLORS;
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
        "color-border": artColors.border,
        ...getArtworkTextColors(artColors)
      };
    }
    return theme.colors;
  }

  async setTheme(themeId: string) {
    if (PREDEFINED_THEMES.some(t => t.id === themeId) || this.customThemes.some(t => t.id === themeId)) {
      this.activeThemeId = themeId;
      this.applyActiveTheme();
      await invoke("set_app_setting", { key: "active_theme_id", value: themeId });
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

    await invoke("set_app_setting", { key: "custom_themes", value: JSON.stringify(this.customThemes) });
    await invoke("set_app_setting", { key: "active_theme_id", value: theme.id });
  }

  async deleteCustomTheme(themeId: string) {
    this.customThemes = this.customThemes.filter(t => t.id !== themeId);
    await invoke("set_app_setting", { key: "custom_themes", value: JSON.stringify(this.customThemes) });

    if (this.activeThemeId === themeId) {
      await this.setTheme("system");
    }
  }

  async importTheme(filePath: string): Promise<Theme> {
    const imported = await invoke<Theme>("import_theme", { filePath });
    const requiredColors: (keyof ThemeColors)[] = [
      "bg-main",
      "bg-sidebar",
      "bg-playerbar",
      "color-accent",
      "color-accent-hover",
      "color-text-primary",
      "color-text-secondary",
      "color-border",
    ];
    for (const key of requiredColors) {
      if (!imported.colors || typeof imported.colors[key] !== "string" || !imported.colors[key].trim()) {
        throw new Error(`Missing or invalid color: ${key}`);
      }
    }
    imported.isCustom = true;
    await this.addCustomTheme(imported);
    return imported;
  }

  async exportTheme(theme: Theme, exportPath: string): Promise<void> {
    await invoke("export_theme", { theme, exportPath });
  }

  async updateArtworkColors(song: Song | undefined) {
    if (!song) {
      this.resetArtworkColors();
      return;
    }

    let url: string | null = null;
    if (song.art_manual) {
      url = resolveArtUrl(song.art_manual);
    } else if (song.art_automatic) {
      url = resolveArtUrl(song.art_automatic);
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
      this.resetArtworkColors();
      return;
    }

    try {
      const colors = await extractColorsFromImage(url);
      this.applyArtworkColors(colors);
    } catch (e) {
      console.error("Failed to extract artwork colors:", e);
      this.resetArtworkColors();
    }
  }

  applyArtworkColors(colors: ExtractedColors, skipApplyActiveTheme = false) {
    this.artworkColors = colors;
    if (typeof document === "undefined") return;
    this.writeArtworkVars(colors, skipApplyActiveTheme);
  }

  resetArtworkColors(skipApplyActiveTheme = false) {
    this.artworkColors = null;
    if (typeof document === "undefined") return;
    this.writeArtworkVars({
      primary: "#2e3440",
      sidebar: "#242933",
      playerbar: "#2b303c",
      accent: "#88c0d0",
      accentHover: "#8fbcbb",
      border: "#3b4252"
    }, skipApplyActiveTheme);
  }

  /**
   * Writes the --color-artwork-* vars. While Dynamic Artwork is active they
   * feed --bg-main etc. directly, so they're written inside the same theme
   * commit as the rest of the theme (see commitThemeChange()) — written
   * before it, they'd already show in the crossfade's "old" snapshot.
   */
  private writeArtworkVars(colors: Omit<ExtractedColors, "isLight">, skipApplyActiveTheme: boolean) {
    const write = () => {
      const root = document.documentElement;
      root.style.setProperty("--color-artwork-primary", colors.primary);
      root.style.setProperty("--color-artwork-sidebar", colors.sidebar);
      root.style.setProperty("--color-artwork-playerbar", colors.playerbar);
      root.style.setProperty("--color-artwork-accent", colors.accent);
      root.style.setProperty("--color-artwork-accent-hover", colors.accentHover);
      root.style.setProperty("--color-artwork-border", colors.border);
    };

    if (!skipApplyActiveTheme && this.activeThemeId === "dynamic-artwork") {
      this.commitThemeChange(() => {
        write();
        this.applyActiveTheme(true);
      });
    } else {
      write();
    }
  }

  applyThemeColorsPreview(colors: ThemeColors) {
    if (typeof document === "undefined") return;

    const resolvedAccent = colors["color-accent"];
    const resolvedAccentHover = colors["color-accent-hover"];
    const resolvedBgMain = colors["bg-main"];
    const resolvedBgSidebar = colors["bg-sidebar"];
    const resolvedBgPlayerbar = colors["bg-playerbar"];

    const accentContrastText = pickAccessibleOnColor(resolvedAccent);
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

    this.applyGlassVars(resolvedBgMain, resolvedBgSidebar, resolvedBgPlayerbar, resolvedAccent);
  }

  /**
   * Glass rendering vars — computed for every theme (not just System) so
   * all four chrome panels get the blur/tint/shine treatment regardless
   * of which theme is active. isDark comes from this theme's own bg-main
   * luminance rather than systemColorScheme, since only System tracks the
   * OS scheme — every other theme has fixed colors. Rendering-only,
   * separate from the opaque theme colors — alpha never reaches a color
   * picker, see hexToRgbaString(). All arguments must be literal hex.
   */
  private applyGlassVars(bgMain: string, bgSidebar: string, bgPlayerbar: string, accent: string) {
    const root = document.documentElement;
    root.classList.toggle("theme-glass", true);

    const isDark = !isLightColor(bgMain);
    const alpha = glassAlpha(isDark);
    root.style.setProperty("--glass-bg-sidebar", hexToRgbaString(bgSidebar, alpha));
    root.style.setProperty("--glass-bg-playerbar", hexToRgbaString(bgPlayerbar, alpha));
    root.style.setProperty("--glass-solid-sidebar", flatGlassColor(bgSidebar, alpha, bgMain));
    root.style.setProperty("--glass-border-color", isDark ? "rgba(255, 255, 255, 0.10)" : "rgba(15, 15, 20, 0.08)");

    const elevation = isDark ? "0 8px 32px rgba(0, 0, 0, 0.45)" : "0 8px 32px rgba(15, 15, 20, 0.10)";
    const highlight = isDark ? "inset 0 1px 0 rgba(255, 255, 255, 0.14)" : "inset 0 1px 0 rgba(255, 255, 255, 0.9)";
    root.style.setProperty("--glass-shadow", `${elevation}, ${highlight}`);

    // PlayDock-only accent glow — kept out of --glass-shadow above since
    // the other three panels don't get it. Two-layer glow (tight bright
    // core + wide soft halo) reads as an actual glow rather than a flat
    // blurred outline. `accent` must be a literal hex: for Dynamic Artwork
    // the theme's color-accent is a CSS var() reference string, and
    // hexToRgbaString() fed that would silently fall back to black,
    // rendering as an invisible glow.
    const glowNear = `0 0 24px 2px ${hexToRgbaString(accent, isDark ? 0.45 : 0.28)}`;
    const glowFar = `0 0 90px 10px ${hexToRgbaString(accent, isDark ? 0.28 : 0.16)}`;
    root.style.setProperty("--glass-glow", `${glowNear}, ${glowFar}`);
  }

  /** Set while a theme change is being written, so nested applies join it. */
  private committingTheme = false;
  private hasAppliedTheme = false;

  /**
   * Writes a theme change to the DOM — as a View Transition crossfade when
   * the webview supports one. A crossfade snapshots the old frame once and
   * fades it into the live new one on the compositor; the fallback
   * @property morph in app.css instead restyles and repaints the entire
   * document (every glass panel included) on every frame of the
   * transition, which staggers on laptop GPUs. The first application at
   * startup and nested applies (applyArtworkColors → applyActiveTheme) are
   * written directly. A newer change arriving mid-crossfade skips the
   * running one, but its DOM writes still run — nothing is lost.
   */
  private commitThemeChange(write: () => void) {
    const run = () => {
      this.committingTheme = true;
      try {
        write();
      } finally {
        this.committingTheme = false;
      }
    };

    const animate = this.hasAppliedTheme && !this.committingTheme && supportsViewTransitions() && !prefersReducedMotion();
    this.hasAppliedTheme = true;
    if (supportsViewTransitions()) {
      // Turns off the @property morph (see app.css) — the crossfade
      // replaces it, and under reduced motion changes apply instantly.
      document.documentElement.classList.add("theme-vt");
    }
    if (animate) {
      yieldViewTransitionToInput(document.startViewTransition(run));
    } else {
      run();
    }
  }

  applyActiveTheme(skipApplyArtworkColors = false) {
    if (typeof document === "undefined") return;
    this.commitThemeChange(() => this.writeActiveTheme(skipApplyArtworkColors));
  }

  private writeActiveTheme(skipApplyArtworkColors: boolean) {
    const theme = this.currentTheme;

    // updateArtworkColors() only re-extracts/applies colors on a song
    // change (see player.svelte.ts), so switching *to* Dynamic Artwork
    // while a song is already playing would otherwise leave the
    // --color-artwork-* CSS vars stale (or unset) until the next track —
    // sync them from whatever's already cached (or the fallback palette)
    // right away instead of waiting for that next update.
    if (!skipApplyArtworkColors && theme.id === "dynamic-artwork") {
      this.applyArtworkColors(this.artworkColors || getFallbackColors(), true);
    }

    const isLuminous = theme.id === "system";
    // The System theme's live colors come from whichever scheme is
    // effectively active (OS preference, or a pinned Light/Dark mode), not
    // the static preview colors on the theme entry.
    const colors = isLuminous
      ? (this.effectiveColorScheme === "dark" ? LUMINOUS_DARK_COLORS : LUMINOUS_LIGHT_COLORS)
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
    const resolvedBgSidebar = theme.id === "dynamic-artwork"
      ? (this.artworkColors || getFallbackColors()).sidebar
      : colors["bg-sidebar"];
    const resolvedBgPlayerbar = theme.id === "dynamic-artwork"
      ? (this.artworkColors || getFallbackColors()).playerbar
      : colors["bg-playerbar"];
    const resolvedAccentHover = theme.id === "dynamic-artwork"
      ? (this.artworkColors || getFallbackColors()).accentHover
      : colors["color-accent-hover"];
    const accentText = clampForContrast(resolvedAccent, resolvedBgMain, 4.5);
    const accentTextHover = clampForContrast(resolvedAccentHover, resolvedBgMain, 4.5);

    const artworkTextColors = theme.id === "dynamic-artwork"
      ? getArtworkTextColors(this.artworkColors || getFallbackColors())
      : null;
    const textPrimary = artworkTextColors ? artworkTextColors["color-text-primary"] : colors["color-text-primary"];
    const textSecondary = artworkTextColors ? artworkTextColors["color-text-secondary"] : colors["color-text-secondary"];

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
        --color-text-primary: ${textPrimary};
        --color-text-secondary: ${textSecondary};
        --color-border: ${colors["color-border"]};
        --color-accent-contrast: ${accentContrastText};
        --color-accent-text: ${accentText};
        --color-accent-text-hover: ${accentTextHover};
      }
    `;

    // resolvedAccent (not colors["color-accent"]) — see applyGlassVars().
    this.applyGlassVars(resolvedBgMain, resolvedBgSidebar, resolvedBgPlayerbar, resolvedAccent);
  }
}

export const themeStore = new ThemeStore();
