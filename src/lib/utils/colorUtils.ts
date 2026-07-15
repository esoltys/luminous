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
