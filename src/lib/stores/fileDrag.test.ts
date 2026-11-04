import { describe, it, expect } from "vitest";
import { fileDrag } from "./fileDrag.svelte";
import type { FileEntry } from "../types";

function mockEntry(name: string, songId: number | null, isDir = false): FileEntry {
  return {
    name,
    isDir,
    size: 1000,
    modified: null,
    fileType: "FLAC",
    kind: "audio",
    opensInApp: false,
    status: songId ? "inLibrary" : "notScanned",
    songId,
    missingTag: null,
    location: `/music/${name}`,
  };
}

describe("fileDrag store", () => {
  it("tracks dragged entries, librarySongIds, and nonLibraryCount", () => {
    expect(fileDrag.isDragging).toBe(false);

    const entries = [
      mockEntry("song1.flac", 101),
      mockEntry("song2.mp3", 102),
      mockEntry("notes.txt", null),
      mockEntry("folder", null, true),
    ];

    fileDrag.start(entries);
    expect(fileDrag.isDragging).toBe(true);
    expect(fileDrag.librarySongIds).toEqual([101, 102]);
    expect(fileDrag.nonLibraryCount).toBe(1); // notes.txt (folder excluded)
    expect(fileDrag.hasPlayableSongs).toBe(true);

    fileDrag.end();
    expect(fileDrag.isDragging).toBe(false);
    expect(fileDrag.draggedEntries).toEqual([]);
    expect(fileDrag.librarySongIds).toEqual([]);
  });
});
