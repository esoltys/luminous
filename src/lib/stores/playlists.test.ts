import { describe, it, expect, beforeEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Playlist, PlaylistItem } from "../types";

import { playlistsStore } from "./playlists.svelte";

describe("PlaylistsStore", () => {
  let eventCallbacks: Record<string, Function> = {};

  const mockPlaylists: Playlist[] = [
    { id: 101, name: "Favorites", track_count: 3, created: 1700000000, dynamic_enabled: false },
    { id: 102, name: "Workout", track_count: 5, created: 1700000001, dynamic_enabled: false }
  ];

  const mockTracks: PlaylistItem[] = [
    {
      id: 1,
      playlist_id: 101,
      position: 0,
      item_type: "song",
      uuid: "item-1",
      song: { id: 10, title: "Energy Song", artist: "Artist A", rating: 4, playcount: 5 } as any
    },
    {
      id: 2,
      playlist_id: 101,
      position: 1,
      item_type: "song",
      uuid: "item-2",
      song: { id: 20, title: "Run Song", artist: "Artist B", rating: 5, playcount: 12 } as any
    }
  ];

  beforeEach(() => {
    vi.clearAllMocks();
    eventCallbacks = {};

    vi.mocked(listen).mockImplementation(async (event: string, callback: any) => {
      eventCallbacks[event] = callback;
      return () => {};
    });

    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: any) => {
      switch (cmd) {
        case "get_playlists":
          return mockPlaylists;
        case "get_playlist_tracks":
          return mockTracks;
        case "get_all_app_settings":
          return { active_playlist_id: "101" };
        case "create_playlist":
          return { id: 103, name: args?.name || "New Playlist", track_count: 0, created_at: "2026-01-03" };
        default:
          return null;
      }
    });
  });

  it("refreshes playlists and selects active playlist", async () => {
    await playlistsStore.refreshPlaylists();
    expect(playlistsStore.playlists).toHaveLength(2);
    expect(playlistsStore.playlists[0].name).toBe("Favorites");

    await playlistsStore.selectPlaylist(101);
    expect(playlistsStore.activePlaylistId).toBe(101);
    expect(playlistsStore.activePlaylistTracks).toHaveLength(2);
    expect(invoke).toHaveBeenCalledWith("get_playlist_tracks", { playlistId: 101 });
    expect(invoke).toHaveBeenCalledWith("set_app_setting", { key: "active_playlist_id", value: "101" });
  });

  it("creates a new playlist and selects it", async () => {
    await playlistsStore.createPlaylist("Chill Beats");

    expect(invoke).toHaveBeenCalledWith("create_playlist", { name: "Chill Beats" });
    expect(playlistsStore.activePlaylistId).toBe(103);
  });

  it("renames a playlist and refreshes playlists list", async () => {
    await playlistsStore.renamePlaylist(101, "Top Favorites");

    expect(invoke).toHaveBeenCalledWith("rename_playlist", { id: 101, name: "Top Favorites" });
    expect(invoke).toHaveBeenCalledWith("get_playlists");
  });

  it("deletes playlist and updates active selection", async () => {
    playlistsStore.activePlaylistId = 101;

    await playlistsStore.deletePlaylist(101);

    expect(invoke).toHaveBeenCalledWith("delete_playlist", { id: 101 });
    expect(playlistsStore.activePlaylistId).toBe(101); // selectPlaylist called with first playlist in list (101)
  });

  it("adds songs to playlist and refreshes tracks if currently active", async () => {
    playlistsStore.activePlaylistId = 101;

    await playlistsStore.addSongsToPlaylist(101, [10, 20, 30]);

    expect(invoke).toHaveBeenCalledWith("add_to_playlist", { playlistId: 101, songIds: [10, 20, 30] });
    expect(invoke).toHaveBeenCalledWith("get_playlist_tracks", { playlistId: 101 });
  });

  it("removes items from playlist and reorders items", async () => {
    playlistsStore.activePlaylistId = 101;

    await playlistsStore.removeItemsFromPlaylist(101, ["item-1"]);
    expect(invoke).toHaveBeenCalledWith("remove_from_playlist", { playlistId: 101, uuids: ["item-1"] });

    await playlistsStore.reorderItem(101, 0, 1);
    expect(invoke).toHaveBeenCalledWith("reorder_playlist_item", { playlistId: 101, from: 0, to: 1 });
  });

  it("clears playlist tracks", async () => {
    playlistsStore.activePlaylistId = 101;
    playlistsStore.activePlaylistTracks = [...mockTracks];

    await playlistsStore.clearPlaylist(101);

    expect(invoke).toHaveBeenCalledWith("clear_playlist", { playlistId: 101 });
    expect(playlistsStore.activePlaylistTracks).toHaveLength(0);
  });

  it("handles undo and redo actions", async () => {
    playlistsStore.activePlaylistId = 101;

    await playlistsStore.undo();
    expect(invoke).toHaveBeenCalledWith("undo_playlist");
    expect(invoke).toHaveBeenCalledWith("get_playlist_tracks", { playlistId: 101 });

    await playlistsStore.redo();
    expect(invoke).toHaveBeenCalledWith("redo_playlist");
    expect(invoke).toHaveBeenCalledWith("get_playlist_tracks", { playlistId: 101 });
  });

  it("updates song stats in activePlaylistTracks on song-stats-changed event", async () => {
    playlistsStore.activePlaylistTracks = JSON.parse(JSON.stringify(mockTracks));

    if (eventCallbacks["song-stats-changed"]) {
      eventCallbacks["song-stats-changed"]({
        payload: { song_id: 10, rating: 5, playcount: 6 }
      });

      expect(playlistsStore.activePlaylistTracks[0].song?.rating).toBe(5);
      expect(playlistsStore.activePlaylistTracks[0].song?.playcount).toBe(6);
    }
  });
});
