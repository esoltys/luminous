import { describe, it, expect } from "vitest";
import { fileDrag, parseDropTarget } from "./fileDrag.svelte";
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

    fileDrag.start(entries, { x: 50, y: 100 });
    expect(fileDrag.isDragging).toBe(true);
    expect(fileDrag.pointerPos).toEqual({ x: 50, y: 100 });
    expect(fileDrag.librarySongIds).toEqual([101, 102]);
    expect(fileDrag.nonLibraryCount).toBe(1); // notes.txt (folder excluded)
    expect(fileDrag.hasPlayableSongs).toBe(true);

    fileDrag.updatePointer({ x: 120, y: 200 }, { kind: "queue" });
    expect(fileDrag.pointerPos).toEqual({ x: 120, y: 200 });
    expect(fileDrag.dropTarget).toEqual({ kind: "queue" });

    fileDrag.end();
    expect(fileDrag.isDragging).toBe(false);
    expect(fileDrag.pointerPos).toBeNull();
    expect(fileDrag.dropTarget).toBeNull();
    expect(fileDrag.draggedEntries).toEqual([]);
    expect(fileDrag.librarySongIds).toEqual([]);
  });

  describe("parseDropTarget", () => {
    it("returns null for null or elements without data-drop-target", () => {
      expect(parseDropTarget(null)).toBeNull();
      const div = document.createElement("div");
      expect(parseDropTarget(div)).toBeNull();
    });

    it("parses queue drop target", () => {
      const btn = document.createElement("button");
      btn.dataset.dropTarget = "queue";
      const icon = document.createElement("span");
      btn.appendChild(icon);

      expect(parseDropTarget(btn)).toEqual({ kind: "queue" });
      expect(parseDropTarget(icon)).toEqual({ kind: "queue" });
    });

    it("parses custom-category drop target", () => {
      const btn = document.createElement("button");
      btn.dataset.dropTarget = "custom-category";
      expect(parseDropTarget(btn)).toEqual({ kind: "custom-category" });
    });

    it("parses playlist drop target with id and name", () => {
      const btn = document.createElement("button");
      btn.dataset.dropTarget = "playlist";
      btn.dataset.playlistId = "42";
      btn.dataset.playlistName = "Road Trip";

      expect(parseDropTarget(btn)).toEqual({ kind: "playlist", id: 42, name: "Road Trip" });
    });

    it("returns null for invalid playlist id", () => {
      const btn = document.createElement("button");
      btn.dataset.dropTarget = "playlist";
      btn.dataset.playlistId = "abc";
      btn.dataset.playlistName = "Road Trip";

      expect(parseDropTarget(btn)).toBeNull();
    });
  });
});
