import { describe, it, expect } from "vitest";
import { parseMultiValue, joinMultiValue, MULTI_VALUE_DELIMITER } from "./multiValue";

describe("multiValue utils", () => {
  describe("parseMultiValue", () => {
    it("splits, trims, and drops empties", () => {
      expect(parseMultiValue("Rock; Jazz Fusion; Live")).toEqual(["Rock", "Jazz Fusion", "Live"]);
      expect(parseMultiValue("Rock;;  Jazz ")).toEqual(["Rock", "Jazz"]);
    });

    it("dedupes case-insensitively, keeping first-seen casing", () => {
      expect(parseMultiValue("Rock; rock; ROCK")).toEqual(["Rock"]);
    });

    it("round-trips a single unsplit value", () => {
      expect(parseMultiValue("Metal")).toEqual(["Metal"]);
    });

    it("returns an empty array for empty/whitespace-only input", () => {
      expect(parseMultiValue("")).toEqual([]);
      expect(parseMultiValue("   ")).toEqual([]);
    });

    it("does not split on '/' — that's only a legacy on-disk fallback, not the storage delimiter", () => {
      expect(parseMultiValue("AC/DC")).toEqual(["AC/DC"]);
    });
  });

  describe("joinMultiValue", () => {
    it("joins with the '; ' delimiter", () => {
      expect(joinMultiValue(["Rock", "Jazz Fusion"])).toBe(`Rock${MULTI_VALUE_DELIMITER}Jazz Fusion`);
    });

    it("returns an empty string for an empty list", () => {
      expect(joinMultiValue([])).toBe("");
    });
  });

  it("round-trips through join and parse", () => {
    const values = ["Rock", "Jazz Fusion"];
    expect(parseMultiValue(joinMultiValue(values))).toEqual(values);
  });
});
