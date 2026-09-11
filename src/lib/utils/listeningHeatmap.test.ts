import { describe, it, expect } from "vitest";
import { bucketDailyMinutes, buildHeatmapGrid, computeStreaks } from "./listeningHeatmap";
import type { ListenEvent } from "../types";

function unixSecondsFor(date: Date): number {
  return Math.floor(date.getTime() / 1000);
}

describe("bucketDailyMinutes", () => {
  it("sums durations within the same local day and floors to whole minutes", () => {
    const day = new Date(2026, 0, 15, 10, 0);
    const events: ListenEvent[] = [
      { played_at: unixSecondsFor(day), duration_secs: 200 },
      { played_at: unixSecondsFor(new Date(2026, 0, 15, 22, 0)), duration_secs: 100 }
    ];
    const result = bucketDailyMinutes(events);
    expect(result.get("2026-01-15")).toBe(5); // (200 + 100) / 60 = 5
  });

  it("keeps different days separate", () => {
    const events: ListenEvent[] = [
      { played_at: unixSecondsFor(new Date(2026, 0, 15, 10, 0)), duration_secs: 180 },
      { played_at: unixSecondsFor(new Date(2026, 0, 16, 10, 0)), duration_secs: 60 }
    ];
    const result = bucketDailyMinutes(events);
    expect(result.get("2026-01-15")).toBe(3);
    expect(result.get("2026-01-16")).toBe(1);
  });
});

describe("buildHeatmapGrid", () => {
  it("produces 7 rows each with `weeks` columns", () => {
    const grid = buildHeatmapGrid(new Map(), 4, "sunday", new Date(2026, 0, 15));
    expect(grid).toHaveLength(7);
    for (const row of grid) expect(row).toHaveLength(4);
  });

  // `new Date("YYYY-MM-DD")` parses as UTC, not local time, so tests parse
  // the cell's date key back into a local Date manually instead.
  function localDayOfWeek(dateKey: string): number {
    const [y, m, d] = dateKey.split("-").map(Number);
    return new Date(y, m - 1, d).getDay();
  }

  it("orders rows Sunday-first when weekStart is sunday", () => {
    // 2026-01-15 is a Thursday.
    const grid = buildHeatmapGrid(new Map(), 1, "sunday", new Date(2026, 0, 15));
    expect(localDayOfWeek(grid[0][0].date)).toBe(0); // Sunday
  });

  it("orders rows Monday-first when weekStart is monday", () => {
    const grid = buildHeatmapGrid(new Map(), 1, "monday", new Date(2026, 0, 15));
    expect(localDayOfWeek(grid[0][0].date)).toBe(1); // Monday
  });

  it("marks days after today (but still within the current week) as future and excludes them from minutes", () => {
    // 2026-01-15 is a Thursday, so with a Sunday-first week, Jan 16-17
    // (Fri/Sat) are still in the grid but haven't happened yet.
    const dailyMinutes = new Map([["2026-01-17", 45]]);
    const grid = buildHeatmapGrid(dailyMinutes, 1, "sunday", new Date(2026, 0, 15));
    const flatCells = grid.flat();
    const futureCell = flatCells.find((c) => c.date === "2026-01-17");
    expect(futureCell?.future).toBe(true);
    expect(futureCell?.minutes).toBe(0);
  });

  it("maps minutes to intensity levels", () => {
    const dailyMinutes = new Map([
      ["2026-01-15", 0],
      ["2026-01-14", 15],
      ["2026-01-13", 45],
      ["2026-01-12", 90],
      ["2026-01-11", 200]
    ]);
    const grid = buildHeatmapGrid(dailyMinutes, 1, "sunday", new Date(2026, 0, 15));
    const byDate = new Map(grid.flat().map((c) => [c.date, c.level]));
    expect(byDate.get("2026-01-15")).toBe(0);
    expect(byDate.get("2026-01-14")).toBe(1);
    expect(byDate.get("2026-01-13")).toBe(2);
    expect(byDate.get("2026-01-12")).toBe(3);
    expect(byDate.get("2026-01-11")).toBe(4);
  });
});

describe("computeStreaks", () => {
  it("returns zero streaks with no listening history", () => {
    expect(computeStreaks(new Map())).toEqual({ current: 0, longest: 0 });
  });

  it("counts a consecutive run ending today", () => {
    const dailyMinutes = new Map([
      ["2026-01-13", 10],
      ["2026-01-14", 10],
      ["2026-01-15", 10]
    ]);
    const result = computeStreaks(dailyMinutes, new Date(2026, 0, 15));
    expect(result.current).toBe(3);
    expect(result.longest).toBe(3);
  });

  it("gives one day of grace when today has no listening yet", () => {
    const dailyMinutes = new Map([
      ["2026-01-13", 10],
      ["2026-01-14", 10]
    ]);
    const result = computeStreaks(dailyMinutes, new Date(2026, 0, 15));
    expect(result.current).toBe(2);
  });

  it("resets current streak after a full missed day", () => {
    const dailyMinutes = new Map([
      ["2026-01-10", 10],
      ["2026-01-15", 10]
    ]);
    const result = computeStreaks(dailyMinutes, new Date(2026, 0, 15));
    expect(result.current).toBe(1);
  });

  it("tracks the longest streak independently of the current one", () => {
    const dailyMinutes = new Map([
      ["2026-01-01", 10],
      ["2026-01-02", 10],
      ["2026-01-03", 10],
      ["2026-01-04", 10],
      ["2026-01-15", 10]
    ]);
    const result = computeStreaks(dailyMinutes, new Date(2026, 0, 15));
    expect(result.current).toBe(1);
    expect(result.longest).toBe(4);
  });
});
