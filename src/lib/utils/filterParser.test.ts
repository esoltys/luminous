import { describe, it, expect } from "vitest";
import { isSmartPlaylistSpec, parseSearchRules } from "./filterParser";

describe("isSmartPlaylistSpec", () => {
  it("returns false for a curated genre auto-playlist spec (tag:)", () => {
    expect(isSmartPlaylistSpec("tag:Metal")).toBe(false);
  });

  it("returns false for a decade auto-playlist spec", () => {
    expect(isSmartPlaylistSpec("decade:1980s")).toBe(false);
  });

  it("returns false for a BPM auto-playlist spec", () => {
    expect(isSmartPlaylistSpec("bpmrange:60-90")).toBe(false);
  });

  it("returns false for an artist tag auto-playlist spec", () => {
    expect(isSmartPlaylistSpec("artisttag:canadian")).toBe(false);
  });

  it("returns false for a null/undefined/empty spec", () => {
    expect(isSmartPlaylistSpec(null)).toBe(false);
    expect(isSmartPlaylistSpec(undefined)).toBe(false);
    expect(isSmartPlaylistSpec("")).toBe(false);
  });

  it("returns true for a user-authored Smart Playlist rule spec", () => {
    expect(isSmartPlaylistSpec("genre:rock")).toBe(true);
    expect(isSmartPlaylistSpec("artist_tag:canadian")).toBe(true);
    expect(isSmartPlaylistSpec("artist:Miles Davis; rating:>=4")).toBe(true);
  });
});

describe("parseSearchRules", () => {
  it("normalizes artist-tag filter to artist_tag with contains operator", () => {
    const rules = parseSearchRules("artist-tag:canadian");
    expect(rules).toEqual([
      { field: "artist_tag", op: "contains", value: "canadian" },
    ]);
  });
});
