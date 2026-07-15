import { describe, it, expect } from "vitest";
import {
  hexToRgb,
  rgbToHex,
  calculateLuminance,
  isLightColor,
  calculateContrastRatio,
  checkWcagCompliance,
  suggestTextColor,
  getColorMetrics,
  formatLuminance,
  getWcagBadgeColor,
  getWcagBadgeText,
  rgbToHsl,
  hslToRgb,
  scoreSwatch,
  ARCHETYPE_TARGETS,
  quantizeMedianCut,
  extractArchetypes,
  type Swatch
} from "./colorUtils";

describe("hexToRgb", () => {
  it("parses a hex string with a leading #", () => {
    expect(hexToRgb("#ff0000")).toEqual({ r: 255, g: 0, b: 0 });
  });

  it("parses a hex string without a leading #", () => {
    expect(hexToRgb("00ff00")).toEqual({ r: 0, g: 255, b: 0 });
  });

  it("falls back to black for an invalid hex string", () => {
    expect(hexToRgb("not-a-color")).toEqual({ r: 0, g: 0, b: 0 });
  });
});

describe("rgbToHex", () => {
  it("converts RGB values to a zero-padded hex string", () => {
    expect(rgbToHex(255, 0, 0)).toBe("#ff0000");
    expect(rgbToHex(0, 5, 255)).toBe("#0005ff");
  });

  it("round-trips with hexToRgb", () => {
    const hex = rgbToHex(139, 92, 246);
    expect(hexToRgb(hex)).toEqual({ r: 139, g: 92, b: 246 });
  });
});

describe("calculateLuminance", () => {
  it("returns 1 for white", () => {
    expect(calculateLuminance("#ffffff")).toBeCloseTo(1, 5);
  });

  it("returns 0 for black", () => {
    expect(calculateLuminance("#000000")).toBe(0);
  });

  it("weights green highest and blue lowest for a fully saturated primary", () => {
    const red = calculateLuminance("#ff0000");
    const green = calculateLuminance("#00ff00");
    const blue = calculateLuminance("#0000ff");
    expect(green).toBeGreaterThan(red);
    expect(red).toBeGreaterThan(blue);
  });
});

describe("isLightColor", () => {
  it("classifies white as light and black as dark", () => {
    expect(isLightColor("#ffffff")).toBe(true);
    expect(isLightColor("#000000")).toBe(false);
  });

  it("uses the 0.179 luminance threshold", () => {
    // #808080 has luminance ~0.216, just above the 0.179 threshold
    expect(isLightColor("#808080")).toBe(true);
    // #666666 has luminance ~0.132, below the threshold
    expect(isLightColor("#666666")).toBe(false);
  });
});

describe("calculateContrastRatio", () => {
  it("returns 21:1 for black vs white (max contrast)", () => {
    expect(calculateContrastRatio("#000000", "#ffffff")).toBeCloseTo(21, 5);
  });

  it("returns 1:1 for identical colors (no contrast)", () => {
    expect(calculateContrastRatio("#8b5cf6", "#8b5cf6")).toBeCloseTo(1, 5);
  });

  it("is symmetric regardless of argument order", () => {
    const a = calculateContrastRatio("#0d0b18", "#f3f4f6");
    const b = calculateContrastRatio("#f3f4f6", "#0d0b18");
    expect(a).toBeCloseTo(b, 10);
  });
});

describe("checkWcagCompliance", () => {
  it("passes AAA for black text on white background", () => {
    const result = checkWcagCompliance("#000000", "#ffffff");
    expect(result.ratio).toBe(21);
    expect(result.wcagAA).toBe(true);
    expect(result.wcagAAA).toBe(true);
    expect(result.level).toBe("AAA");
  });

  it("fails for low-contrast pairs", () => {
    const result = checkWcagCompliance("#9ca3af", "#a0a5ac");
    expect(result.wcagAA).toBe(false);
    expect(result.wcagAAA).toBe(false);
    expect(result.level).toBe("fail");
  });

  it("classifies AA-only contrast correctly (>=4.5 but <7)", () => {
    // Chosen so ratio lands in the AA band, not AAA
    const result = checkWcagCompliance("#767676", "#ffffff");
    expect(result.ratio).toBeGreaterThanOrEqual(4.5);
    expect(result.ratio).toBeLessThan(7);
    expect(result.level).toBe("AA");
    expect(result.wcagAA).toBe(true);
    expect(result.wcagAAA).toBe(false);
  });
});

describe("suggestTextColor", () => {
  it("suggests dark text on light backgrounds", () => {
    expect(suggestTextColor("#ffffff")).toBe("#1a1a1a");
  });

  it("suggests light text on dark backgrounds", () => {
    expect(suggestTextColor("#000000")).toBe("#ffffff");
  });
});

describe("getColorMetrics", () => {
  it("returns full metrics for white", () => {
    const metrics = getColorMetrics("#ffffff");
    expect(metrics.hex).toBe("#ffffff");
    expect(metrics.rgb).toEqual({ r: 255, g: 255, b: 255 });
    expect(metrics.luminance).toBeCloseTo(1, 5);
    expect(metrics.isLight).toBe(true);
    expect(metrics.linearRgb.r).toBeCloseTo(1, 5);
  });

  it("returns full metrics for black", () => {
    const metrics = getColorMetrics("#000000");
    expect(metrics.luminance).toBe(0);
    expect(metrics.isLight).toBe(false);
    expect(metrics.linearRgb).toEqual({ r: 0, g: 0, b: 0 });
  });
});

describe("formatLuminance", () => {
  it("formats luminance as a rounded percentage", () => {
    expect(formatLuminance(1)).toBe("100%");
    expect(formatLuminance(0)).toBe("0%");
    expect(formatLuminance(0.5)).toBe("50%");
  });
});

describe("getWcagBadgeColor", () => {
  it("returns green for AAA, amber for AA, red for fail", () => {
    expect(getWcagBadgeColor("AAA")).toBe("#10b981");
    expect(getWcagBadgeColor("AA")).toBe("#f59e0b");
    expect(getWcagBadgeColor("fail")).toBe("#ef4444");
  });
});

describe("getWcagBadgeText", () => {
  it("returns the expected label for each level", () => {
    expect(getWcagBadgeText("AAA")).toBe("✓ WCAG AAA");
    expect(getWcagBadgeText("AA")).toBe("✓ WCAG AA");
    expect(getWcagBadgeText("fail")).toBe("✗ Below AA");
  });
});

describe("rgbToHsl", () => {
  it("converts pure red to 0deg hue, full saturation, 50% lightness", () => {
    expect(rgbToHsl(255, 0, 0)).toEqual({ h: 0, s: 1, l: 0.5 });
  });

  it("converts white to 0 saturation, 100% lightness", () => {
    expect(rgbToHsl(255, 255, 255)).toEqual({ h: 0, s: 0, l: 1 });
  });

  it("converts black to 0 saturation, 0% lightness", () => {
    expect(rgbToHsl(0, 0, 0)).toEqual({ h: 0, s: 0, l: 0 });
  });

  it("converts a neon blue accent to high saturation, mid lightness", () => {
    const hsl = rgbToHsl(30, 60, 255);
    expect(hsl.s).toBeGreaterThan(0.8);
    expect(hsl.l).toBeGreaterThan(0.4);
    expect(hsl.l).toBeLessThan(0.6);
  });
});

describe("hslToRgb", () => {
  it("round-trips with rgbToHsl for a saturated color", () => {
    const hsl = rgbToHsl(30, 60, 255);
    const rgb = hslToRgb(hsl.h, hsl.s, hsl.l);
    expect(rgb.r).toBeCloseTo(30, 0);
    expect(rgb.g).toBeCloseTo(60, 0);
    expect(rgb.b).toBeCloseTo(255, 0);
  });

  it("converts 0 saturation to a gray regardless of hue", () => {
    expect(hslToRgb(200, 0, 0.5)).toEqual({ r: 128, g: 128, b: 128 });
  });

  it("converts full lightness to white and zero lightness to black", () => {
    expect(hslToRgb(120, 0.8, 1)).toEqual({ r: 255, g: 255, b: 255 });
    expect(hslToRgb(120, 0.8, 0)).toEqual({ r: 0, g: 0, b: 0 });
  });
});

describe("scoreSwatch", () => {
  it("scores 0 for a swatch outside the archetype's saturation guard rail", () => {
    // A pure gray (s=0) can never be VIBRANT (minS 0.35) no matter its population
    const gray = { h: 0, s: 0, l: 0.5 };
    expect(scoreSwatch(gray, 100, 100, ARCHETYPE_TARGETS.vibrant)).toBe(0);
  });

  it("scores higher for a swatch closer to the archetype's target saturation", () => {
    const close = { h: 200, s: 0.9, l: 0.5 };
    const far = { h: 200, s: 0.4, l: 0.5 };
    const scoreClose = scoreSwatch(close, 50, 100, ARCHETYPE_TARGETS.vibrant);
    const scoreFar = scoreSwatch(far, 50, 100, ARCHETYPE_TARGETS.vibrant);
    expect(scoreClose).toBeGreaterThan(scoreFar);
  });

  it("gives a higher-population swatch a higher score, all else equal", () => {
    const hsl = { h: 200, s: 0.9, l: 0.5 };
    const scoreLow = scoreSwatch(hsl, 10, 100, ARCHETYPE_TARGETS.vibrant);
    const scoreHigh = scoreSwatch(hsl, 90, 100, ARCHETYPE_TARGETS.vibrant);
    expect(scoreHigh).toBeGreaterThan(scoreLow);
  });
});

describe("quantizeMedianCut", () => {
  it("returns a single swatch spanning the whole population for a flat-color input", () => {
    const swatches = quantizeMedianCut([{ r: 10, g: 10, b: 10, count: 500 }], 8);
    expect(swatches).toEqual([{ r: 10, g: 10, b: 10, population: 500 }]);
  });

  it("keeps a tiny neon accent as its own swatch instead of averaging it into a huge black background", () => {
    // A moody-rock-album stand-in: near-black dominates by population,
    // a small neon-blue cluster is the only accent — this is the exact
    // case flat population-dominance quantization loses.
    const swatches = quantizeMedianCut(
      [
        { r: 5, g: 5, b: 5, count: 1000 },
        { r: 20, g: 40, b: 255, count: 5 }
      ],
      8
    );
    const neon = swatches.find(s => s.b > 200);
    expect(neon).toBeDefined();
    expect(neon?.population).toBe(5);
    const black = swatches.find(s => s.b <= 10);
    expect(black?.population).toBe(1000);
  });

  it("conserves total population across all output swatches", () => {
    const input = [
      { r: 5, g: 5, b: 5, count: 1000 },
      { r: 20, g: 40, b: 255, count: 5 },
      { r: 200, g: 200, b: 200, count: 50 },
      { r: 128, g: 64, b: 32, count: 20 }
    ];
    const swatches = quantizeMedianCut(input, 8);
    const totalIn = input.reduce((sum, c) => sum + c.count, 0);
    const totalOut = swatches.reduce((sum, s) => sum + s.population, 0);
    expect(totalOut).toBe(totalIn);
  });

  it("never returns more swatches than requested", () => {
    const input = Array.from({ length: 40 }, (_, i) => ({
      r: (i * 6) % 256,
      g: (i * 11) % 256,
      b: (i * 17) % 256,
      count: 1
    }));
    const swatches = quantizeMedianCut(input, 8);
    expect(swatches.length).toBeLessThanOrEqual(8);
  });

  it("returns an empty array for empty input", () => {
    expect(quantizeMedianCut([], 8)).toEqual([]);
  });
});

describe("extractArchetypes", () => {
  it("picks the neon accent as vibrant, not the dominant black background", () => {
    const swatches = quantizeMedianCut(
      [
        { r: 5, g: 5, b: 5, count: 1000 },
        { r: 20, g: 40, b: 255, count: 5 }
      ],
      8
    );
    const archetypes = extractArchetypes(swatches);
    expect(archetypes.vibrant).toBeTruthy();
    expect(archetypes.vibrant?.b).toBeGreaterThan(200);
  });

  it("falls back vibrant to the dominant-by-population swatch when nothing clears the guard rails", () => {
    // Two flat grays (s=0): neither can satisfy any archetype's minS
    const swatches: Swatch[] = [
      { r: 200, g: 200, b: 200, population: 100 },
      { r: 10, g: 10, b: 10, population: 900 }
    ];
    const archetypes = extractArchetypes(swatches);
    expect(archetypes.vibrant).toEqual({ r: 10, g: 10, b: 10, population: 900 });
  });

  it("returns null for an archetype no candidate satisfies", () => {
    const swatches: Swatch[] = [{ r: 10, g: 10, b: 10, population: 900 }];
    const archetypes = extractArchetypes(swatches);
    expect(archetypes.lightVibrant).toBeNull();
  });
});
