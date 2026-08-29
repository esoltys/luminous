import { describe, it, expect, beforeEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({
    onResized: vi.fn(() => Promise.resolve(() => {})),
    onMoved: vi.fn(() => Promise.resolve(() => {})),
  })),
}));

import { collectionStore } from "./collection.svelte";

describe("CollectionStore - directories, scanning, and library stats", () => {
  let eventCallbacks: Record<string, Function> = {};

  beforeEach(() => {
    vi.clearAllMocks();
    eventCallbacks = {};

    vi.mocked(listen).mockImplementation(async (event: string, callback: any) => {
      eventCallbacks[event] = callback;
      return () => {};
    });

    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case "geometry_capture_supported":
          return true;
        case "get_directories":
          return [{ id: 1, path: "/music/rock", created_at: "2026-01-01" }];
        case "get_library_stats":
          return {
            total_songs: 10,
            total_artists: 2,
            total_albums: 2,
            total_duration_nanosec: 30000000000,
            total_filesize_bytes: 50000000
          };
        case "get_library_snapshot":
          return {
            songs: [
              { id: 1, title: "Rock Track 1", artist: "Rock Band", album: "Rock Album", filetype: "MP3" },
              { id: 2, title: "Jazz Track 1", artist: "Jazz Quartet", album: "Jazz Album", filetype: "FLAC" },
              { id: 3, title: "Vorbis Track", artist: "Indie Group", album: "Indie Album", filetype: "OGG_VORBIS" }
            ],
            albums: [
              { album: "Rock Album", artist: "Rock Band", song_count: 5, year: 2020 },
              { album: "Jazz Album", artist: "Jazz Quartet", song_count: 5, year: 2021 }
            ],
            artists: [
              { name: "Rock Band", album_count: 1, song_count: 5 },
              { name: "Jazz Quartet", album_count: 1, song_count: 5 }
            ]
          };
        case "get_all_app_settings":
          return {};
        default:
          return null;
      }
    });
  });

  it("refreshes directories, stats, and library upon refresh calls", async () => {
    await collectionStore.refreshDirectories();
    expect(collectionStore.directories).toHaveLength(1);
    expect(collectionStore.directories[0].path).toBe("/music/rock");

    await collectionStore.refreshStats();
    expect(collectionStore.stats.total_songs).toBe(10);

    await collectionStore.refreshLibrary();
    expect(collectionStore.songs).toHaveLength(3);
    expect(collectionStore.albums).toHaveLength(2);
    expect(collectionStore.artists).toHaveLength(2);
  });

  it("invokes backend on addDirectory and removeDirectory", async () => {
    await collectionStore.addDirectory("/music/pop");
    expect(invoke).toHaveBeenCalledWith("add_directory", { path: "/music/pop" });
    expect(invoke).toHaveBeenCalledWith("get_directories");

    await collectionStore.removeDirectory("/music/pop");
    expect(invoke).toHaveBeenCalledWith("remove_directory", { path: "/music/pop" });
  });

  it("handles directory scanning and scan-progress event with force option", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(undefined as any);
    await collectionStore.startScan(true);
    expect(collectionStore.isScanning).toBe(true);
    expect(invoke).toHaveBeenCalledWith("scan_directories", { force: true });

    if (eventCallbacks["scan-progress"]) {
      eventCallbacks["scan-progress"]({
        payload: { phase: "reading_tags", current_path: "song.mp3", scanned: 5, total: 10 }
      });
      expect(collectionStore.scanProgress?.scanned).toBe(5);
      expect(collectionStore.isScanning).toBe(true);

      eventCallbacks["scan-progress"]({
        payload: { phase: "done", current_path: "", scanned: 10, total: 10 }
      });
      expect(collectionStore.isScanning).toBe(false);
      expect(collectionStore.lastScanTime).not.toBeNull();
    }
  });

  it("handles pruneMissing songs call", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ deleted_songs: 3, removed_folders: 2, merged_duplicates: 1 } as any);
    const result = await collectionStore.pruneMissing();
    expect(invoke).toHaveBeenCalledWith("prune_missing_songs");
    expect(result).toEqual({ deletedSongs: 3, removedFolders: 2, mergedDuplicates: 1 });
  });

  it("toggles and persists watchFoldersRealtime and scanOnStartup settings", async () => {
    await collectionStore.setWatchFoldersRealtime(false);
    expect(collectionStore.watchFoldersRealtime).toBe(false);
    expect(invoke).toHaveBeenCalledWith("set_app_setting", { key: "watch_folders_realtime", value: "false" });

    await collectionStore.setScanOnStartup(true);
    expect(collectionStore.scanOnStartup).toBe(true);
    expect(invoke).toHaveBeenCalledWith("set_app_setting", { key: "scan_on_startup", value: "true" });
  });
});
