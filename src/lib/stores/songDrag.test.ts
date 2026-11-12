import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import {
  songDrag,
  filesPayload,
  songsPayload,
  parseDropTarget,
  findDropTargetAt,
} from "./songDrag.svelte";
import { playlistsStore } from "./playlists.svelte";
import { toastStore } from "./toast.svelte";
import type { FileEntry, Song } from "../types";

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

const song = (id: number, title: string) => ({ id, title }) as Song;

/** Puts a sized drop target in the page, so `move` finds it under the pointer at (50, 40). */
function placeTarget(attrs: Record<string, string>): HTMLElement {
  const el = document.createElement("button");
  Object.assign(el.dataset, attrs);
  el.getBoundingClientRect = () =>
    ({ left: 10, right: 110, top: 20, bottom: 60, width: 100, height: 40 }) as DOMRect;
  document.body.appendChild(el);
  return el;
}

const toastTexts = () => toastStore.messages.map((m) => m.text);

describe("songDrag store", () => {
  beforeEach(() => {
    songDrag.end();
    toastStore.messages = [];
    playlistsStore.playlists = [
      { id: 1, name: "Queue", is_queue: true } as any,
      { id: 2, name: "Road Trip", is_queue: false } as any,
    ];
  });
  afterEach(() => {
    document.body.innerHTML = "";
    vi.restoreAllMocks();
  });

  describe("payloads", () => {
    it("carries library files from the File Browser and counts the rest as skipped", () => {
      const p = filesPayload([
        mockEntry("song1.flac", 101),
        mockEntry("song2.mp3", 102),
        mockEntry("notes.txt", null),
        mockEntry("folder", null, true),
      ]);
      expect(p.songIds).toEqual([101, 102]);
      expect(p.skippedCount).toBe(1); // notes.txt; folders are ignored
      expect(p.label).toBe("4 songs");
      expect(p.sourcePlaylistId).toBeNull();
      expect(filesPayload([mockEntry("song1.flac", 101)]).label).toBe("song1.flac");
    });

    it("carries songs from a song list, labelled by title or count", () => {
      expect(songsPayload([song(7, "Karma Police")], 2)).toEqual({
        songIds: [7],
        label: "Karma Police",
        skippedCount: 0,
        sourcePlaylistId: 2,
      });
      expect(songsPayload([song(7, "A"), song(8, "B")]).label).toBe("2 songs");
    });
  });

  describe("move", () => {
    it("tracks the pointer and the target under it", () => {
      placeTarget({ dropTarget: "playlist", playlistId: "2", playlistName: "Road Trip" });
      songDrag.start(songsPayload([song(7, "A")]), { x: 0, y: 0 });

      songDrag.move({ x: 50, y: 40 });
      expect(songDrag.pointerPos).toEqual({ x: 50, y: 40 });
      expect(songDrag.dropTarget).toEqual({ kind: "playlist", id: 2, name: "Road Trip" });

      songDrag.move({ x: 300, y: 300 });
      expect(songDrag.dropTarget).toBeNull();
    });

    it("never offers the playlist the songs came from", () => {
      placeTarget({ dropTarget: "playlist", playlistId: "2", playlistName: "Road Trip" });
      songDrag.start(songsPayload([song(7, "A")], 2), { x: 0, y: 0 });
      songDrag.move({ x: 50, y: 40 });
      expect(songDrag.dropTarget).toBeNull();
    });

    it("never offers the Queue for songs dragged out of the Queue", () => {
      placeTarget({ dropTarget: "queue" });
      songDrag.start(songsPayload([song(7, "A")], 1), { x: 0, y: 0 });
      songDrag.move({ x: 50, y: 40 });
      expect(songDrag.dropTarget).toBeNull();

      // ...but songs from another playlist can go to the Queue.
      songDrag.start(songsPayload([song(7, "A")], 2), { x: 0, y: 0 });
      songDrag.move({ x: 50, y: 40 });
      expect(songDrag.dropTarget).toEqual({ kind: "queue" });
    });
  });

  describe("drop", () => {
    it("adds the songs to the Queue and says how many", async () => {
      const add = vi.spyOn(playlistsStore, "addSongsToQueue").mockResolvedValue();
      placeTarget({ dropTarget: "queue" });
      songDrag.start(songsPayload([song(7, "A"), song(8, "B")]), { x: 0, y: 0 });

      expect(await songDrag.drop({ x: 50, y: 40 })).toBe(true);
      expect(add).toHaveBeenCalledWith([7, 8]);
      expect(toastTexts()).toEqual(["Added 2 songs to queue"]);
      expect(songDrag.isDragging).toBe(false);
    });

    it("adds the songs to a playlist and names it", async () => {
      const add = vi.spyOn(playlistsStore, "addSongsToPlaylist").mockResolvedValue();
      placeTarget({ dropTarget: "playlist", playlistId: "2", playlistName: "Road Trip" });
      songDrag.start(songsPayload([song(7, "A")]), { x: 0, y: 0 });

      expect(await songDrag.drop({ x: 50, y: 40 })).toBe(true);
      expect(add).toHaveBeenCalledWith(2, [7]);
      expect(toastTexts()).toEqual(["Added to Road Trip"]);
    });

    it("reports files left out because they aren't in the library", async () => {
      vi.spyOn(playlistsStore, "addSongsToQueue").mockResolvedValue();
      placeTarget({ dropTarget: "queue" });
      songDrag.start(filesPayload([mockEntry("a.flac", 101), mockEntry("b.flac", null)]), {
        x: 0,
        y: 0,
      });

      await songDrag.drop({ x: 50, y: 40 });
      expect(toastTexts()).toHaveLength(2);
      expect(toastTexts()[0]).toBe("Added 1 song to queue");
    });

    it("says the add failed, naming the playlist", async () => {
      vi.spyOn(console, "error").mockImplementation(() => {});
      vi.spyOn(playlistsStore, "addSongsToPlaylist").mockRejectedValue(new Error("db locked"));
      placeTarget({ dropTarget: "playlist", playlistId: "2", playlistName: "Road Trip" });
      songDrag.start(songsPayload([song(7, "A")]), { x: 0, y: 0 });

      expect(await songDrag.drop({ x: 50, y: 40 })).toBe(true);
      expect(toastTexts()).toEqual(["Couldn't add to Road Trip."]);
    });

    it("adds nothing when released away from a target, so the caller can fall back", async () => {
      const add = vi.spyOn(playlistsStore, "addSongsToQueue").mockResolvedValue();
      placeTarget({ dropTarget: "queue" });
      songDrag.start(songsPayload([song(7, "A")]), { x: 0, y: 0 });

      expect(await songDrag.drop({ x: 300, y: 300 })).toBe(false);
      expect(add).not.toHaveBeenCalled();
      expect(toastTexts()).toEqual([]);
      expect(songDrag.isDragging).toBe(false);
    });

    it("adds nothing when released on the playlist the songs came from", async () => {
      const add = vi.spyOn(playlistsStore, "addSongsToPlaylist").mockResolvedValue();
      placeTarget({ dropTarget: "playlist", playlistId: "2", playlistName: "Road Trip" });
      songDrag.start(songsPayload([song(7, "A")], 2), { x: 0, y: 0 });

      expect(await songDrag.drop({ x: 50, y: 40 })).toBe(false);
      expect(add).not.toHaveBeenCalled();
    });
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

  describe("findDropTargetAt", () => {
    it("finds drop target using geometric bounding rect", () => {
      const btn = document.createElement("button");
      btn.dataset.dropTarget = "queue";
      document.body.appendChild(btn);

      btn.getBoundingClientRect = () =>
        ({
          left: 10,
          right: 110,
          top: 20,
          bottom: 60,
          width: 100,
          height: 40,
        }) as DOMRect;

      expect(findDropTargetAt(50, 40)).toEqual({ kind: "queue" });
      expect(findDropTargetAt(5, 40)).toBeNull();
      expect(findDropTargetAt(150, 40)).toBeNull();

      document.body.removeChild(btn);
    });
  });
});
