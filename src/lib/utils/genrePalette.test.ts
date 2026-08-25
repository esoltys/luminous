import { describe, it, expect } from "vitest";
import { resolveGenreColorIndex } from "./genrePalette";
import type { TagGroup } from "../types";

describe("resolveGenreColorIndex", () => {
  const hierarchy: TagGroup[] = [
    {
      name: "Metal",
      color_index: 3,
      song_count: 10,
      children: [{ name: "Progressive Metal", song_count: 4 }],
    },
    {
      name: "Ambient",
      color_index: 7,
      song_count: 5,
      children: [],
    },
  ];

  it("resolves a top-level card's own color", () => {
    expect(resolveGenreColorIndex(hierarchy, "Metal")).toBe(3);
  });

  it("resolves a chip's color via its parent card", () => {
    expect(resolveGenreColorIndex(hierarchy, "Progressive Metal")).toBe(3);
  });

  it("returns undefined for a name not present anywhere in the hierarchy", () => {
    expect(resolveGenreColorIndex(hierarchy, "Unknown Genre")).toBeUndefined();
  });
});
