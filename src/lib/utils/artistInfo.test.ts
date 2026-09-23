import { describe, it, expect } from "vitest";
import {
  formatArtistDate,
  getYearsAgo,
  formatRelativeYears,
  formatArtistLifeEvent,
  isArtistPerson,
  isArtistGroup,
  resolveGenderLabel,
  getArtistAreaLinks,
} from "./artistInfo";

describe("artistInfo utils", () => {
  describe("formatArtistDate", () => {
    it("formats full YYYY-MM-DD date", () => {
      expect(formatArtistDate("1965-08-28", "en")).toBe("August 28, 1965");
    });

    it("formats partial YYYY-MM date", () => {
      expect(formatArtistDate("1994-04", "en")).toBe("April 1994");
    });

    it("formats year-only YYYY", () => {
      expect(formatArtistDate("1987", "en")).toBe("1987");
    });

    it("handles empty or invalid strings gracefully", () => {
      expect(formatArtistDate(null)).toBe("");
      expect(formatArtistDate("")).toBe("");
      expect(formatArtistDate("unknown")).toBe("unknown");
    });
  });

  describe("getYearsAgo", () => {
    const fixedNow = new Date("2026-09-22T00:00:00Z");

    it("computes full years elapsed for past full date when birthday has passed", () => {
      // 1965-08-28 to 2026-09-22: birthday passed in August, so 2026 - 1965 = 61
      expect(getYearsAgo("1965-08-28", fixedNow)).toBe(61);
    });

    it("computes full years elapsed for past full date when birthday has not passed yet", () => {
      // 1965-10-15 to 2026-09-22: birthday in October, so 2026 - 1965 - 1 = 60
      expect(getYearsAgo("1965-10-15", fixedNow)).toBe(60);
    });

    it("computes full years elapsed for year-only date", () => {
      // 1987 to 2026: 2026 - 1987 = 39
      expect(getYearsAgo("1987", fixedNow)).toBe(39);
    });

    it("computes full years elapsed for year-month date", () => {
      // 1994-04 to 2026-09: April passed, so 32
      expect(getYearsAgo("1994-04", fixedNow)).toBe(32);
      // 1994-11 to 2026-09: November has not passed, so 31
      expect(getYearsAgo("1994-11", fixedNow)).toBe(31);
    });

    it("returns null for invalid strings", () => {
      expect(getYearsAgo(null)).toBeNull();
      expect(getYearsAgo("")).toBeNull();
      expect(getYearsAgo("invalid")).toBeNull();
    });
  });

  describe("formatRelativeYears", () => {
    it("returns 1 year ago for single year", () => {
      expect(formatRelativeYears(1)).toBe("1 year ago");
    });

    it("returns N years ago for multiple years", () => {
      expect(formatRelativeYears(61)).toBe("61 years ago");
    });

    it("handles zero or negative years", () => {
      expect(formatRelativeYears(0)).toBe("<1 year ago");
    });
  });

  describe("formatArtistLifeEvent", () => {
    const fixedNow = new Date("2026-09-22T00:00:00Z");

    it("combines absolute date and relative duration", () => {
      expect(formatArtistLifeEvent("1965-08-28", "en", fixedNow)).toBe(
        "August 28, 1965 (61 years ago)"
      );
    });

    it("combines year-only date and relative duration", () => {
      expect(formatArtistLifeEvent("1987", "en", fixedNow)).toBe("1987 (39 years ago)");
    });
  });

  describe("isArtistPerson & isArtistGroup", () => {
    it("identifies person type", () => {
      expect(isArtistPerson("Person", null)).toBe(true);
      expect(isArtistPerson("person", null)).toBe(true);
      expect(isArtistPerson(null, "Female")).toBe(true);
      expect(isArtistPerson("Group", "Female")).toBe(false);
    });

    it("identifies group type", () => {
      expect(isArtistGroup("Group")).toBe(true);
      expect(isArtistGroup("group")).toBe(true);
      expect(isArtistGroup("Person")).toBe(false);
      expect(isArtistGroup(null)).toBe(false);
    });
  });

  describe("resolveGenderLabel", () => {
    it("resolves recognized genders", () => {
      expect(resolveGenderLabel("Female")).toBe("Female");
      expect(resolveGenderLabel("female")).toBe("Female");
      expect(resolveGenderLabel("Male")).toBe("Male");
      expect(resolveGenderLabel("male")).toBe("Male");
      expect(resolveGenderLabel("non-binary")).toBe("Non-binary");
      expect(resolveGenderLabel("other")).toBe("Other");
    });

    it("handles unknown or empty genders", () => {
      expect(resolveGenderLabel(null)).toBe("");
      expect(resolveGenderLabel("")).toBe("");
      expect(resolveGenderLabel("custom")).toBe("Custom");
    });
  });

  describe("getArtistAreaLinks", () => {
    it("returns both begin area and area when different", () => {
      const links = getArtistAreaLinks(
        "Windsor",
        "area-uuid-1",
        "Canada",
        "area-uuid-2"
      );
      expect(links).toEqual([
        { name: "Windsor", url: "https://musicbrainz.org/area/area-uuid-1" },
        { name: "Canada", url: "https://musicbrainz.org/area/area-uuid-2" },
      ]);
    });

    it("deduplicates when begin area and area are identical", () => {
      const links = getArtistAreaLinks(
        "Singapore",
        "area-uuid-sg",
        "Singapore",
        "area-uuid-sg"
      );
      expect(links).toEqual([
        { name: "Singapore", url: "https://musicbrainz.org/area/area-uuid-sg" },
      ]);
    });

    it("handles missing mbids gracefully", () => {
      const links = getArtistAreaLinks("Windsor", null, "Canada", null);
      expect(links).toEqual([
        { name: "Windsor", url: undefined },
        { name: "Canada", url: undefined },
      ]);
    });

    it("handles only country present", () => {
      const links = getArtistAreaLinks(null, null, "Canada", "area-uuid-ca");
      expect(links).toEqual([
        { name: "Canada", url: "https://musicbrainz.org/area/area-uuid-ca" },
      ]);
    });
  });
});
