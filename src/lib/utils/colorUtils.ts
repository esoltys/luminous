// CIE Color Science and Accessibility Utilities
// Based on International Commission on Illumination (CIE) standards

export interface ColorMetrics {
  hex: string;
  rgb: { r: number; g: number; b: number };
  linearRgb: { r: number; g: number; b: number };
  luminance: number;
  isLight: boolean;
}

export interface ContrastResult {
  ratio: number;
  wcagAA: boolean;
  wcagAAA: boolean;
  level: 'fail' | 'AA' | 'AAA';
}

/**
 * Convert hex color to RGB
 * @param hex Hex color string (#RRGGBB)
 * @returns RGB values 0-255
 */
export function hexToRgb(hex: string): { r: number; g: number; b: number } {
  const result = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i.exec(hex);
  return result
    ? {
        r: parseInt(result[1], 16),
        g: parseInt(result[2], 16),
        b: parseInt(result[3], 16)
      }
    : { r: 0, g: 0, b: 0 };
}

/**
 * Convert RGB to hex color
 * @param r Red value 0-255
 * @param g Green value 0-255
 * @param b Blue value 0-255
 * @returns Hex color string (#RRGGBB)
 */
export function rgbToHex(r: number, g: number, b: number): string {
  const toHex = (c: number) => {
    const hex = c.toString(16);
    return hex.length === 1 ? "0" + hex : hex;
  };
  return `#${toHex(r)}${toHex(g)}${toHex(b)}`;
}

/**
 * Convert sRGB to Linear RGB
 * Undoes gamma correction to get true physical light measurements
 *
 * For values < 0.03928: linear transformation
 * For values >= 0.03928: exponential transformation with exponent 2.4
 * This matches how monitors actually display colors
 */
function sRgbToLinearRgb(value: number): number {
  const normalized = value / 255;
  if (normalized <= 0.03928) {
    return normalized / 12.92;
  }
  return Math.pow((normalized + 0.055) / 1.055, 2.4);
}

/**
 * Convert Linear RGB to sRGB
 */
function linearRgbToSRgb(value: number): number {
  if (value <= 0.0031308) {
    return value * 12.92 * 255;
  }
  return (1.055 * Math.pow(value, 1 / 2.4) - 0.055) * 255;
}

/**
 * Calculate relative luminance according to WCAG 2.0 standard
 * Uses CIE color weights that match human eye sensitivity
 *
 * Green: 71.52% weight (humans most sensitive to green)
 * Red: 21.26% weight
 * Blue: 7.22% weight (humans least sensitive to blue)
 *
 * Returns value 0.0 (black) to 1.0 (white)
 */
export function calculateLuminance(hex: string): number {
  const rgb = hexToRgb(hex);

  const r = sRgbToLinearRgb(rgb.r);
  const g = sRgbToLinearRgb(rgb.g);
  const b = sRgbToLinearRgb(rgb.b);

  // CIE relative luminance formula
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/**
 * Determine if a color is perceptually light or dark
 * Uses geometric mean of black (0.0) and white (1.0) as threshold
 * sqrt(0.05 × 1.05) ≈ 0.179
 *
 * This is the mathematical tipping point for perfect contrast
 */
export function isLightColor(hex: string): boolean {
  return calculateLuminance(hex) > 0.179;
}

/**
 * Calculate contrast ratio between two colors
 * Formula: (L1 + 0.05) / (L2 + 0.05), where L1 is lighter color
 *
 * Returns value 1 (no contrast) to 21 (maximum contrast)
 */
export function calculateContrastRatio(hex1: string, hex2: string): number {
  const lum1 = calculateLuminance(hex1);
  const lum2 = calculateLuminance(hex2);

  const lighter = Math.max(lum1, lum2);
  const darker = Math.min(lum1, lum2);

  return (lighter + 0.05) / (darker + 0.05);
}

/**
 * Check WCAG contrast compliance
 *
 * WCAG AA: 4.5:1 for normal text, 3:1 for large text
 * WCAG AAA: 7:1 for normal text, 4.5:1 for large text
 */
export function checkWcagCompliance(foreground: string, background: string): ContrastResult {
  const ratio = calculateContrastRatio(foreground, background);

  return {
    ratio: Math.round(ratio * 100) / 100,
    wcagAA: ratio >= 4.5,
    wcagAAA: ratio >= 7,
    level: ratio >= 7 ? 'AAA' : ratio >= 4.5 ? 'AA' : 'fail'
  };
}

/**
 * Suggest appropriate text color (dark or light) based on background
 * Uses the 0.179 luminance threshold
 */
export function suggestTextColor(backgroundColor: string): string {
  return isLightColor(backgroundColor) ? '#1a1a1a' : '#ffffff';
}

/**
 * Picks whichever of pure white or pure black has higher contrast against
 * a background — the best possible binary choice by definition, unlike
 * suggestTextColor()'s fixed luminance threshold. Used where a color is
 * heuristically derived (e.g. text rendered directly on an arbitrary,
 * possibly user-chosen, accent color) rather than hand-picked per theme,
 * so it can't silently fail WCAG AA the way a fixed choice could.
 */
export function pickAccessibleOnColor(backgroundColor: string): string {
  const whiteContrast = calculateContrastRatio('#ffffff', backgroundColor);
  const blackContrast = calculateContrastRatio('#000000', backgroundColor);
  return whiteContrast >= blackContrast ? '#ffffff' : '#000000';
}

/**
 * Get full color metrics for a hex color
 */
export function getColorMetrics(hex: string): ColorMetrics {
  const rgb = hexToRgb(hex);
  const luminance = calculateLuminance(hex);

  return {
    hex,
    rgb,
    linearRgb: {
      r: sRgbToLinearRgb(rgb.r),
      g: sRgbToLinearRgb(rgb.g),
      b: sRgbToLinearRgb(rgb.b)
    },
    luminance,
    isLight: luminance > 0.179
  };
}

/**
 * Format luminance as percentage for display
 */
export function formatLuminance(luminance: number): string {
  return `${Math.round(luminance * 100)}%`;
}

/**
 * Get WCAG level badge color
 */
export function getWcagBadgeColor(level: 'fail' | 'AA' | 'AAA'): string {
  switch (level) {
    case 'AAA':
      return '#10b981'; // green
    case 'AA':
      return '#f59e0b'; // amber
    case 'fail':
      return '#ef4444'; // red
  }
}

/**
 * Get WCAG level badge text
 */
export function getWcagBadgeText(level: 'fail' | 'AA' | 'AAA'): string {
  switch (level) {
    case 'AAA':
      return '✓ WCAG AAA';
    case 'AA':
      return '✓ WCAG AA';
    case 'fail':
      return '✗ Below AA';
  }
}

export interface HSL {
  h: number; // degrees, 0-360
  s: number; // 0-1
  l: number; // 0-1
}

/**
 * Convert RGB (0-255) to HSL. Used to score candidate swatches against the
 * Android Palette-style archetypes below — HSL's saturation/lightness axes
 * map directly onto "vibrant vs muted" and "light vs dark" in a way RGB
 * doesn't.
 */
export function rgbToHsl(r: number, g: number, b: number): HSL {
  r /= 255;
  g /= 255;
  b /= 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;

  if (max === min) {
    return { h: 0, s: 0, l };
  }

  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  let h: number;
  switch (max) {
    case r:
      h = (g - b) / d + (g < b ? 6 : 0);
      break;
    case g:
      h = (b - r) / d + 2;
      break;
    default:
      h = (r - g) / d + 4;
      break;
  }
  return { h: (h / 6) * 360, s, l };
}

/**
 * Inverse of rgbToHsl. Used to derive a family of related shades (surface
 * background, sidebar, border, accent hover, ...) from a single seed color
 * by stepping lightness in HSL space while holding hue/saturation fixed —
 * unlike naive RGB brightness multiplication, this can't drift the hue.
 */
export function hslToRgb(h: number, s: number, l: number): { r: number; g: number; b: number } {
  if (s === 0) {
    const v = Math.round(l * 255);
    return { r: v, g: v, b: v };
  }

  const hueToRgb = (p: number, q: number, t: number): number => {
    let tt = t;
    if (tt < 0) tt += 1;
    if (tt > 1) tt -= 1;
    if (tt < 1 / 6) return p + (q - p) * 6 * tt;
    if (tt < 1 / 2) return q;
    if (tt < 2 / 3) return p + (q - p) * (2 / 3 - tt) * 6;
    return p;
  };

  const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
  const p = 2 * l - q;
  const hNorm = h / 360;

  return {
    r: Math.round(hueToRgb(p, q, hNorm + 1 / 3) * 255),
    g: Math.round(hueToRgb(p, q, hNorm) * 255),
    b: Math.round(hueToRgb(p, q, hNorm - 1 / 3) * 255)
  };
}

/** One of the six Android Palette / Spotify-style UI color roles. */
export type Archetype = 'vibrant' | 'lightVibrant' | 'darkVibrant' | 'muted' | 'lightMuted' | 'darkMuted';

interface ArchetypeTarget {
  targetS: number;
  minS: number;
  maxS: number;
  targetL: number;
  minL: number;
  maxL: number;
}

/**
 * Aesthetic benchmarks each archetype is scored against. Guard rails
 * (min/max) reject a candidate outright; target values shape the score of
 * whatever passes the guard rails.
 */
export const ARCHETYPE_TARGETS: Record<Archetype, ArchetypeTarget> = {
  vibrant: { targetS: 1.0, minS: 0.35, maxS: 1.0, targetL: 0.5, minL: 0.3, maxL: 0.7 },
  lightVibrant: { targetS: 1.0, minS: 0.35, maxS: 1.0, targetL: 0.74, minL: 0.55, maxL: 1.0 },
  darkVibrant: { targetS: 1.0, minS: 0.35, maxS: 1.0, targetL: 0.26, minL: 0.0, maxL: 0.45 },
  muted: { targetS: 0.3, minS: 0.0, maxS: 0.4, targetL: 0.5, minL: 0.3, maxL: 0.7 },
  lightMuted: { targetS: 0.3, minS: 0.0, maxS: 0.4, targetL: 0.74, minL: 0.55, maxL: 1.0 },
  darkMuted: { targetS: 0.3, minS: 0.0, maxS: 0.4, targetL: 0.26, minL: 0.0, maxL: 0.45 }
};

const WEIGHT_SATURATION = 0.6;
const WEIGHT_LUMINANCE = 0.3;
const WEIGHT_POPULATION = 0.1;

/**
 * Scores how well a candidate swatch fits an archetype: 0 if it falls
 * outside the archetype's saturation/lightness guard rails, otherwise a
 * weighted blend of closeness-to-target and population (so a huge but
 * off-target background can't outscore a smaller, better-matching swatch —
 * the exact failure mode that made naive dominant-color extraction lose
 * small vibrant accents on mostly-black covers).
 */
export function scoreSwatch(hsl: HSL, population: number, maxPopulation: number, target: ArchetypeTarget): number {
  const { s, l } = hsl;
  if (s < target.minS || s > target.maxS || l < target.minL || l > target.maxL) {
    return 0;
  }
  const satScore = (1 - Math.abs(s - target.targetS)) * WEIGHT_SATURATION;
  const lumScore = (1 - Math.abs(l - target.targetL)) * WEIGHT_LUMINANCE;
  const popScore = (population / maxPopulation) * WEIGHT_POPULATION;
  return satScore + lumScore + popScore;
}

export interface ColorCount {
  r: number;
  g: number;
  b: number;
  count: number;
}

export interface Swatch {
  r: number;
  g: number;
  b: number;
  population: number;
}

type RgbChannel = 'r' | 'g' | 'b';

function widestChannel(box: ColorCount[]): { axis: RgbChannel; range: number } {
  let minR = 255, maxR = 0, minG = 255, maxG = 0, minB = 255, maxB = 0;
  for (const c of box) {
    if (c.r < minR) minR = c.r;
    if (c.r > maxR) maxR = c.r;
    if (c.g < minG) minG = c.g;
    if (c.g > maxG) maxG = c.g;
    if (c.b < minB) minB = c.b;
    if (c.b > maxB) maxB = c.b;
  }
  const ranges: Record<RgbChannel, number> = { r: maxR - minR, g: maxG - minG, b: maxB - minB };
  const axis = (Object.keys(ranges) as RgbChannel[]).reduce((a, b) => (ranges[b] > ranges[a] ? b : a));
  return { axis, range: ranges[axis] };
}

/**
 * Median Cut color quantization: reduces a weighted color histogram down to
 * at most `maxSwatches` representative colors. Chosen over flat/K-Means
 * bucketing because each split is driven by color *range*, not population —
 * a box containing both a huge near-black background and a handful of
 * neon-blue pixels has a huge range on the blue channel, so it keeps
 * getting split until the neon cluster is isolated into its own swatch,
 * rather than being averaged away into "dark gray".
 */
export function quantizeMedianCut(colors: ColorCount[], maxSwatches: number): Swatch[] {
  if (colors.length === 0) return [];

  const boxes: ColorCount[][] = [colors.slice()];

  while (boxes.length < maxSwatches) {
    let splitIdx = -1;
    let splitAxis: RgbChannel = 'r';
    let widestRange = 0;

    for (let i = 0; i < boxes.length; i++) {
      if (boxes[i].length <= 1) continue;
      const { axis, range } = widestChannel(boxes[i]);
      if (range > widestRange) {
        widestRange = range;
        splitIdx = i;
        splitAxis = axis;
      }
    }

    if (splitIdx === -1) break; // no box left with more than one distinct color

    const box = boxes[splitIdx];
    box.sort((a, b) => a[splitAxis] - b[splitAxis]);
    const totalCount = box.reduce((sum, c) => sum + c.count, 0);

    let cumulative = 0;
    let cutAt = 1;
    for (let i = 0; i < box.length; i++) {
      cumulative += box[i].count;
      if (cumulative >= totalCount / 2) {
        cutAt = i + 1;
        break;
      }
    }
    cutAt = Math.min(Math.max(cutAt, 1), box.length - 1);

    boxes.splice(splitIdx, 1, box.slice(0, cutAt), box.slice(cutAt));
  }

  return boxes
    .map(box => {
      let rSum = 0, gSum = 0, bSum = 0, population = 0;
      for (const c of box) {
        rSum += c.r * c.count;
        gSum += c.g * c.count;
        bSum += c.b * c.count;
        population += c.count;
      }
      return {
        r: Math.round(rSum / population),
        g: Math.round(gSum / population),
        b: Math.round(bSum / population),
        population
      };
    })
    .sort((a, b) => b.population - a.population);
}

/**
 * Evaluates every candidate swatch against all six archetype targets and
 * returns the best match for each. If no candidate clears "vibrant"'s
 * guard rails at all (e.g. an entirely desaturated cover), vibrant falls
 * back to the dominant-by-population swatch so the UI always has an accent
 * rather than nothing. Other archetypes are left null when unmatched —
 * callers chain their own role-appropriate fallbacks (e.g. darkVibrant →
 * darkMuted → dominant) since "no good light-vibrant swatch" isn't
 * necessarily a problem the way "no accent at all" is.
 */
export function extractArchetypes(swatches: Swatch[]): Record<Archetype, Swatch | null> {
  const result = {} as Record<Archetype, Swatch | null>;
  if (swatches.length === 0) {
    for (const key of Object.keys(ARCHETYPE_TARGETS) as Archetype[]) result[key] = null;
    return result;
  }

  const maxPopulation = Math.max(...swatches.map(s => s.population));

  for (const [key, target] of Object.entries(ARCHETYPE_TARGETS) as [Archetype, ArchetypeTarget][]) {
    let best: Swatch | null = null;
    let bestScore = 0;
    for (const swatch of swatches) {
      const hsl = rgbToHsl(swatch.r, swatch.g, swatch.b);
      const score = scoreSwatch(hsl, swatch.population, maxPopulation, target);
      if (score > bestScore) {
        bestScore = score;
        best = swatch;
      }
    }
    result[key] = best;
  }

  if (!result.vibrant) {
    result.vibrant = swatches.reduce((max, s) => (s.population > max.population ? s : max), swatches[0]);
  }

  return result;
}

/** The 8-value palette shape every predefined/custom theme uses (structurally identical to theme.svelte.ts's ThemeColors — kept separate here to avoid a circular import). */
export interface GeneratedThemePalette {
  "bg-main": string;
  "bg-sidebar": string;
  "bg-playerbar": string;
  "color-accent": string;
  "color-accent-hover": string;
  "color-text-primary": string;
  "color-text-secondary": string;
  "color-border": string;
}

const BG_MAIN_LIGHTNESS = 0.12;
const BG_SIDEBAR_DELTA = -0.045;
const BG_PLAYERBAR_DELTA = -0.02;
const BORDER_DELTA = 0.13;
const ACCENT_HOVER_DELTA = 0.12;

function toHex(rgb: { r: number; g: number; b: number }): string {
  return rgbToHex(rgb.r, rgb.g, rgb.b);
}

/**
 * Derives a full 8-value theme palette from a single seed color using HSL
 * lightness/saturation steps, validated against the same WCAG thresholds
 * the hand-picked Luminous palettes already use: 4.5:1 ("normal text") for
 * both text colors against every background surface, and WCAG 1.4.11's
 * 3:1 ("non-text/UI-component") threshold for the accent against bg-main —
 * the same trade-off `LUMINOUS_LIGHT_ACCENT` documents, since the accent is
 * used almost entirely as icon/button/badge color, not paragraph text.
 *
 * The background family is tinted toward the seed's hue at low saturation
 * (a moody near-black, not pure gray) rather than derived from the accent's
 * exact lightness, mirroring the relationship already present across Ruby
 * Red / Nordic Blue / Retro Amber: bg-sidebar darkest, bg-playerbar
 * between it and bg-main, border lighter than bg-main.
 */
export function generatePaletteFromSeed(seedHex: string): GeneratedThemePalette {
  const seedHsl = rgbToHsl(...(Object.values(hexToRgb(seedHex)) as [number, number, number]));

  const bgSaturation = Math.min(seedHsl.s * 0.35, 0.25);
  const bgMain = { h: seedHsl.h, s: bgSaturation, l: BG_MAIN_LIGHTNESS };
  const bgSidebar = { h: seedHsl.h, s: bgSaturation, l: Math.max(0, BG_MAIN_LIGHTNESS + BG_SIDEBAR_DELTA) };
  const bgPlayerbar = { h: seedHsl.h, s: bgSaturation, l: Math.max(0, BG_MAIN_LIGHTNESS + BG_PLAYERBAR_DELTA) };
  const border = { h: seedHsl.h, s: bgSaturation, l: Math.min(1, BG_MAIN_LIGHTNESS + BORDER_DELTA) };

  const accentSaturation = Math.max(seedHsl.s, 0.55);
  const accentLightness = Math.min(Math.max(seedHsl.l, 0.45), 0.65);
  const accent = { h: seedHsl.h, s: accentSaturation, l: accentLightness };
  const accentHover = { h: seedHsl.h, s: accentSaturation, l: Math.min(0.85, accentLightness + ACCENT_HOVER_DELTA) };

  let textPrimary = { h: seedHsl.h, s: 0.15, l: 0.97 };
  let textSecondary = { h: seedHsl.h, s: 0.12, l: 0.82 };

  const backgroundHexes = [toHex(hslToRgb(bgMain.h, bgMain.s, bgMain.l)), toHex(hslToRgb(bgSidebar.h, bgSidebar.s, bgSidebar.l)), toHex(hslToRgb(bgPlayerbar.h, bgPlayerbar.s, bgPlayerbar.l))];

  const meetsAA = (hsl: HSL) => {
    const hex = toHex(hslToRgb(hsl.h, hsl.s, hsl.l));
    return backgroundHexes.every(bg => checkWcagCompliance(hex, bg).wcagAA);
  };

  for (let i = 0; i < 20 && !meetsAA(textPrimary); i++) {
    textPrimary = { ...textPrimary, l: Math.min(1, textPrimary.l + 0.02) };
  }
  for (let i = 0; i < 20 && !meetsAA(textSecondary); i++) {
    textSecondary = { ...textSecondary, l: Math.min(1, textSecondary.l + 0.02) };
  }

  let accentAdjusted = accent;
  for (let i = 0; i < 20; i++) {
    const accentHex = toHex(hslToRgb(accentAdjusted.h, accentAdjusted.s, accentAdjusted.l));
    if (checkWcagCompliance(accentHex, backgroundHexes[0]).ratio >= 3) break;
    accentAdjusted = { ...accentAdjusted, l: Math.min(0.85, accentAdjusted.l + 0.02) };
  }

  return {
    "bg-main": backgroundHexes[0],
    "bg-sidebar": backgroundHexes[1],
    "bg-playerbar": backgroundHexes[2],
    "color-accent": toHex(hslToRgb(accentAdjusted.h, accentAdjusted.s, accentAdjusted.l)),
    "color-accent-hover": toHex(hslToRgb(accentHover.h, accentHover.s, accentHover.l)),
    "color-text-primary": toHex(hslToRgb(textPrimary.h, textPrimary.s, textPrimary.l)),
    "color-text-secondary": toHex(hslToRgb(textSecondary.h, textSecondary.s, textSecondary.l)),
    "color-border": toHex(hslToRgb(border.h, border.s, border.l))
  };
}
