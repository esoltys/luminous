import { describe, it, expect } from "vitest";
import { parseGenres, joinGenres, GENRE_DELIMITER } from "./genre";

describe("genre utils", () => {
  describe("parseGenres", () => {
    it("splits, trims, and drops empties", () => {
      expect(parseGenres("Rock; Jazz Fusion; Live")).toEqual(["Rock", "Jazz Fusion", "Live"]);
      expect(parseGenres("Rock;;  Jazz ")).toEqual(["Rock", "Jazz"]);
    });

    it("dedupes case-insensitively, keeping first-seen casing", () => {
      expect(parseGenres("Rock; rock; ROCK")).toEqual(["Rock"]);
    });

    it("round-trips a single unsplit genre", () => {
      expect(parseGenres("Metal")).toEqual(["Metal"]);
    });

    it("returns an empty array for empty/whitespace-only input", () => {
      expect(parseGenres("")).toEqual([]);
      expect(parseGenres("   ")).toEqual([]);
    });

    it("does not split on '/' — that's only a legacy on-disk fallback, not the storage delimiter", () => {
      expect(parseGenres("Hip-Hop/Rap")).toEqual(["Hip-Hop/Rap"]);
    });
  });

  describe("joinGenres", () => {
    it("joins with the '; ' delimiter", () => {
      expect(joinGenres(["Rock", "Jazz Fusion"])).toBe(`Rock${GENRE_DELIMITER}Jazz Fusion`);
    });

    it("returns an empty string for an empty list", () => {
      expect(joinGenres([])).toBe("");
    });
  });

  it("round-trips through join and parse", () => {
    const genres = ["Rock", "Jazz Fusion"];
    expect(parseGenres(joinGenres(genres))).toEqual(genres);
  });
});
