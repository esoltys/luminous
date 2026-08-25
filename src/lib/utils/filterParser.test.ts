import { describe, it, expect } from "vitest";
import { isSmartPlaylistSpec } from "./filterParser";

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

  it("returns false for a null/undefined/empty spec", () => {
    expect(isSmartPlaylistSpec(null)).toBe(false);
    expect(isSmartPlaylistSpec(undefined)).toBe(false);
    expect(isSmartPlaylistSpec("")).toBe(false);
  });

  it("returns true for a user-authored Smart Playlist rule spec", () => {
    expect(isSmartPlaylistSpec("genre:rock")).toBe(true);
    expect(isSmartPlaylistSpec("artist:Miles Davis; rating:>=4")).toBe(true);
  });
});
