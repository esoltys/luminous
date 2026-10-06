import { describe, it, expect } from "vitest";
import { statsBarPercents } from "./statsBars";

describe("statsBarPercents", () => {
  it("returns an empty list for no items", () => {
    expect(statsBarPercents([])).toEqual([]);
  });

  it("scales minutes against the leader", () => {
    expect(statsBarPercents([{ minutes: 80, play_count: 0 }, { minutes: 40, play_count: 0 }])).toEqual([100, 50]);
  });

  it("falls back to play counts when the leader has no minutes", () => {
    expect(statsBarPercents([{ minutes: 0, play_count: 10 }, { minutes: 0, play_count: 5 }])).toEqual([100, 50]);
  });

  it("returns zeros when nothing has a metric", () => {
    expect(statsBarPercents([{ minutes: 0, play_count: 0 }])).toEqual([0]);
  });
});
