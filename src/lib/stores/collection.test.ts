import { describe, it, expect, beforeEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Song, AlbumItem, ArtistItem } from "../types";

import { collectionStore } from "./collection.svelte";

describe("CollectionStore", () => {
  let eventCallbacks: Record<string, Function> = {};

  beforeEach(() => {
    vi.clearAllMocks();
    eventCallbacks = {};

    vi.mocked(listen).mockImplementation(async (event: string, callback: any) => {
      eventCallbacks[event] = callback;
      return () => {};
    });

    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: any) => {
      switch (cmd) {
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
        case "get_songs":
          return [
            { id: 1, title: "Rock Track 1", artist: "Rock Band", album: "Rock Album", filetype: "MP3" },
            { id: 2, title: "Jazz Track 1", artist: "Jazz Quartet", album: "Jazz Album", filetype: "FLAC" },
            { id: 3, title: "Vorbis Track", artist: "Indie Group", album: "Indie Album", filetype: "OGG_VORBIS" }
          ];
        case "get_albums":
          return [
            { album: "Rock Album", artist: "Rock Band", song_count: 5, year: 2020 },
            { album: "Jazz Album", artist: "Jazz Quartet", song_count: 5, year: 2021 }
          ];
        case "get_artists":
          return [
            { name: "Rock Band", album_count: 1, song_count: 5 },
            { name: "Jazz Quartet", album_count: 1, song_count: 5 }
          ];
        case "get_all_app_settings":
          return { excluded_formats: JSON.stringify(["FLAC"]) };
        case "search_songs":
          if (args?.query === "Rock") {
            return [{ id: 1, title: "Rock Track 1", artist: "Rock Band", album: "Rock Album", filetype: "MP3" }];
          }
          return [];
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
    vi.mocked(invoke).mockResolvedValueOnce(3 as any);
    const count = await collectionStore.pruneMissing();
    expect(invoke).toHaveBeenCalledWith("prune_missing_songs");
    expect(count).toBe(3);
  });

  it("toggles and persists watchFoldersRealtime and scanOnStartup settings", async () => {
    await collectionStore.setWatchFoldersRealtime(false);
    expect(collectionStore.watchFoldersRealtime).toBe(false);
    expect(invoke).toHaveBeenCalledWith("set_app_setting", { key: "watch_folders_realtime", value: "false" });

    await collectionStore.setScanOnStartup(true);
    expect(collectionStore.scanOnStartup).toBe(true);
    expect(invoke).toHaveBeenCalledWith("set_app_setting", { key: "scan_on_startup", value: "true" });
  });

  it("executes FTS search and updates search results and loading state", async () => {
    await collectionStore.search("Rock");
    expect(invoke).toHaveBeenCalledWith("search_songs", { query: "Rock", limit: 500 });
    expect(collectionStore.searchQuery).toBe("Rock");
    expect(collectionStore.searchResults).toHaveLength(1);
    expect(collectionStore.searchResults[0].title).toBe("Rock Track 1");

    await collectionStore.search("");
    expect(collectionStore.searchResults).toHaveLength(0);
  });

  it("filters songs based on format exclusions", () => {
    collectionStore.songs = [
      { id: 1, title: "Song 1", filetype: "MP3" } as Song,
      { id: 2, title: "Song 2", filetype: "FLAC" } as Song,
      { id: 3, title: "Song 3", filetype: "OGG_VORBIS" } as Song
    ];

    collectionStore.excludedFormats = ["FLAC"];
    expect(collectionStore.isFormatExcluded("FLAC")).toBe(true);
    expect(collectionStore.isFormatExcluded("MP3")).toBe(false);

    let filtered = collectionStore.filteredSongs;
    expect(filtered).toHaveLength(2);
    expect(filtered.some(s => s.filetype === "FLAC")).toBe(false);

    collectionStore.excludedFormats = ["OGG"];
    expect(collectionStore.isFormatExcluded("OGG_VORBIS")).toBe(true);
  });

  it("toggles excluded formats and persists setting", async () => {
    collectionStore.excludedFormats = ["MP3"];

    await collectionStore.toggleFormat("FLAC");
    expect(collectionStore.excludedFormats).toEqual(["MP3", "FLAC"]);
    expect(invoke).toHaveBeenCalledWith("set_app_setting", {
      key: "excluded_formats",
      value: JSON.stringify(["MP3", "FLAC"])
    });

    await collectionStore.toggleFormat("MP3");
    expect(collectionStore.excludedFormats).toEqual(["FLAC"]);
  });

  it("filters albums and artists by search query", () => {
    collectionStore.albums = [
      { album: "Dark Side", artist: "Pink Floyd" } as AlbumItem,
      { album: "Abbey Road", artist: "The Beatles" } as AlbumItem
    ];
    collectionStore.artists = [
      { name: "Pink Floyd" } as ArtistItem,
      { name: "The Beatles" } as ArtistItem
    ];

    collectionStore.searchQuery = "beatles";
    expect(collectionStore.filteredAlbums).toHaveLength(1);
    expect(collectionStore.filteredAlbums[0].album).toBe("Abbey Road");

    expect(collectionStore.filteredArtists).toHaveLength(1);
    expect(collectionStore.filteredArtists[0].name).toBe("The Beatles");
  });

  it("handles navigation helpers viewArtist and viewAlbum and clears search terms", () => {
    collectionStore.searchQuery = "some search";
    collectionStore.searchResults = [{ id: 1 } as Song];

    collectionStore.viewArtist("Pink Floyd");
    expect(collectionStore.selectedArtistName).toBe("Pink Floyd");
    expect(collectionStore.activeTab).toBe("collection");
    expect(collectionStore.activeSubTab).toBe("artists");
    expect(collectionStore.searchQuery).toBe("");
    expect(collectionStore.searchResults).toHaveLength(0);

    collectionStore.searchQuery = "another search";
    collectionStore.viewAlbum("Dark Side");
    expect(collectionStore.selectedAlbumName).toBe("Dark Side");
    expect(collectionStore.searchQuery).toBe("");

    collectionStore.navigateTo("playlists");
    expect(collectionStore.activeTab).toBe("playlists");
    expect(collectionStore.selectedArtistName).toBeNull();
  });

  it("persists the selected album/artist detail view to localStorage so a relaunch restores it", () => {
    collectionStore.viewAlbum("Dark Side of the Moon");
    expect(localStorage.getItem("navigation_selectedAlbumName")).toBe("Dark Side of the Moon");
    expect(localStorage.getItem("navigation_selectedArtistName")).toBeNull();

    collectionStore.viewArtist("Pink Floyd");
    expect(localStorage.getItem("navigation_selectedArtistName")).toBe("Pink Floyd");
    expect(localStorage.getItem("navigation_selectedAlbumName")).toBeNull();

    collectionStore.selectedArtistName = null;
    expect(localStorage.getItem("navigation_selectedArtistName")).toBeNull();
  });

  it("toggles and persists layout states (sidebar, right panel, immersive mode)", () => {
    const initialSidebar = collectionStore.sidebarOpen;
    collectionStore.toggleSidebar();
    expect(collectionStore.sidebarOpen).toBe(!initialSidebar);

    collectionStore.setSidebarWidth(300);
    expect(collectionStore.sidebarWidth).toBe(300);

    const initialRight = collectionStore.rightPanelOpen;
    collectionStore.toggleRightPanel();
    expect(collectionStore.rightPanelOpen).toBe(!initialRight);

    collectionStore.setRightPanelWidth(320);
    expect(collectionStore.rightPanelWidth).toBe(320);

    collectionStore.immersiveMode = true;
    collectionStore.exitImmersiveMode();
    expect(collectionStore.immersiveMode).toBe(false);
  });
});
