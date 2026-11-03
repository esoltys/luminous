import { describe, expect, it } from "vitest";
import { albumKey, sameAlbum } from "./albumKey";
import fixture from "./albumKey.fixture.json";

describe("albumKey", () => {
  // The Rust `album_key` runs the same fixture (src-tauri/src/album_key.rs).
  it.each(fixture)("folds $input like the backend", ({ input, key }) => {
    expect(albumKey(input)).toBe(key);
  });

  it("treats a missing title as blank", () => {
    expect(albumKey(null)).toBe("");
    expect(albumKey(undefined)).toBe("");
  });

  it("matches titles differing only in case or surrounding whitespace", () => {
    expect(sameAlbum("Songs of the Sea", "Songs Of The Sea ")).toBe(true);
    expect(sameAlbum("Songs of the Sea", "Songs of the Seas")).toBe(false);
  });

  it("separates albums with the same title by artist (#1932)", () => {
    const kacey = albumKey("Golden Hour", "Kacey Musgraves");
    const balcony = albumKey("Golden Hour", "Balcony Sunrise");
    expect(kacey).not.toBe(balcony);
    expect(kacey).toBe("kacey musgraves\x1fgolden hour");
    expect(balcony).toBe("balcony sunrise\x1fgolden hour");
  });

  it("assigns compilation prefix [va] to compilations (#1932)", () => {
    expect(albumKey("Now 50", "Various Artists", true)).toBe("[va]\x1fnow 50");
    expect(albumKey("Now 50", "Queen", true)).toBe("[va]\x1fnow 50");
    expect(albumKey("Now 50", "Various Artists", false)).toBe("[va]\x1fnow 50");
    expect(albumKey("Now 50", "V.A.", false)).toBe("[va]\x1fnow 50");
  });
});
