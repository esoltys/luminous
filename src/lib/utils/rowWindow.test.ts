import { describe, it, expect } from "vitest";
import { rowTops, visibleRows } from "./rowWindow";

describe("rowWindow", () => {
  it("places each row after the ones above it, ending at the total height", () => {
    expect(rowTops([10, 20, 30])).toEqual([0, 10, 30, 60]);
    expect(rowTops([])).toEqual([0]);
  });

  it("returns only the rows overlapping the span, plus overscan", () => {
    const tops = rowTops(new Array(1000).fill(50));
    expect(visibleRows(tops, 0, 500, 0)).toEqual({ start: 0, end: 10 });
    expect(visibleRows(tops, 25_000, 25_500, 0)).toEqual({ start: 500, end: 510 });
    expect(visibleRows(tops, 25_010, 25_510, 0)).toEqual({ start: 500, end: 511 });
    expect(visibleRows(tops, 25_000, 25_500, 5)).toEqual({ start: 495, end: 515 });
  });

  it("finds rows by their real offsets when heights differ", () => {
    // Row 1 carries a divider, so it is taller and row 2 starts later.
    const tops = rowTops([50, 80, 50, 50]);
    expect(visibleRows(tops, 130, 180, 0)).toEqual({ start: 2, end: 3 });
    expect(visibleRows(tops, 60, 70, 0)).toEqual({ start: 1, end: 2 });
  });

  it("clamps spans that fall outside the list", () => {
    const tops = rowTops([50, 50, 50]);
    expect(visibleRows(tops, -500, 60, 10)).toEqual({ start: 0, end: 3 });
    expect(visibleRows(tops, 10_000, 10_500, 1)).toEqual({ start: 2, end: 3 });
    expect(visibleRows(rowTops([]), 0, 500, 10)).toEqual({ start: 0, end: 0 });
  });
});
