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
});
