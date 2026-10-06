import { describe, it, expect } from "vitest";
import { computeMosaicLayout } from "./mosaicLayout";

describe("computeMosaicLayout", () => {
  it("returns null when there are no quarter covers", () => {
    expect(computeMosaicLayout({ width: 400, height: 200, quarterCount: 0 })).toBeNull();
  });

  it("reproduces the legacy layout with unbounded width and 2 rows / 4 cols", () => {
    const one = computeMosaicLayout({ width: Infinity, height: 100, quarterCount: 1 })!;
    expect([one.cols, one.rows, one.unit, one.shown]).toEqual([3, 2, 49, 1]);
    const two = computeMosaicLayout({ width: Infinity, height: 100, quarterCount: 2 })!;
    expect([two.cols, two.shown]).toEqual([3, 2]);
    const four = computeMosaicLayout({ width: Infinity, height: 100, quarterCount: 4 })!;
    expect([four.cols, four.rows, four.width]).toEqual([4, 2, 202]);
    const many = computeMosaicLayout({ width: Infinity, height: 100, quarterCount: 9 })!;
    expect(many.shown).toBe(4);
  });

  it("adds a third row when the box is too narrow for more columns", () => {
    const l = computeMosaicLayout({ width: 400, height: 400, quarterCount: 7, maxRows: 4, maxCols: 8 })!;
    expect(l.rows).toBeGreaterThanOrEqual(3);
    expect(l.shown).toBe(7);
  });

  it("extends horizontally when there is spare width", () => {
    const l = computeMosaicLayout({ width: 700, height: 144, quarterCount: 7, maxRows: 4, maxCols: 8, minTile: 60 })!;
    expect(l.rows).toBe(2);
    expect(l.cols).toBe(6);
    expect(l.shown).toBe(7);
  });

  it("never exceeds the box", () => {
    for (const [w, h, n] of [[300, 300, 12], [900, 200, 12], [250, 600, 9], [1000, 1000, 15]] as const) {
      const l = computeMosaicLayout({ width: w, height: h, quarterCount: n, maxRows: 4, maxCols: 8 })!;
      expect(l.width).toBeLessThanOrEqual(w + 0.001);
      expect(l.height).toBeLessThanOrEqual(h + 0.001);
    }
  });

  it("respects the minimum tile edge", () => {
    const l = computeMosaicLayout({ width: 300, height: 150, quarterCount: 12, maxRows: 4, maxCols: 8, minTile: 60 })!;
    expect(l.unit).toBeGreaterThanOrEqual(60);
  });

  it("fits a Top 10 (hero + 9) in a tall narrow box", () => {
    const l = computeMosaicLayout({ width: 290, height: 431, quarterCount: 9, maxRows: 4, maxCols: 8, minTile: 43 })!;
    expect(l.shown).toBe(9);
    // 1 big (3x3) + 9 singles = a perfect 6x3 grid, nothing dangling.
    expect([l.heroSpan, l.cols, l.rows]).toEqual([3, 6, 3]);
  });

  it("prefers an exactly filled grid over one with empty cells of similar size", () => {
    const l = computeMosaicLayout({ width: 600, height: 400, quarterCount: 5, maxRows: 4, maxCols: 8 })!;
    expect(l.cols * l.rows - l.heroSpan ** 2).toBe(5);
  });
});
