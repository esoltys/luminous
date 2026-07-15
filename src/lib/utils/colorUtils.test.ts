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
  getWcagBadgeText
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
