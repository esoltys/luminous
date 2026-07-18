import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Playlist, PlaylistItem } from "../types";
import { applySongStats, type SongStatsPayload } from "../utils/stats";

class PlaylistsStore {
  playlists = $state<Playlist[]>([]);
  activePlaylistId = $state<number | null>(null);
  activePlaylistTracks = $state<PlaylistItem[]>([]);

  constructor() {
    this.init();
  }

  private async init() {
    try {
      // Keep loaded playlist tracks in sync with rating/playcount changes
      // made anywhere in the app.
      await listen<SongStatsPayload>("song-stats-changed", (event) => {
        for (const item of this.activePlaylistTracks) {
          if (item.song?.id === event.payload.song_id) {
            applySongStats(item.song, event.payload);
          }
        }
      });

      await this.refreshPlaylists();
      const settings = await invoke<Record<string, string>>("get_all_app_settings");
      if (settings && settings.active_playlist_id) {
        const plId = parseInt(settings.active_playlist_id, 10);
        if (!isNaN(plId) && this.playlists.some((p) => p.id === plId)) {
          await this.selectPlaylist(plId);
          return;
        }
      }
      if (this.playlists.length > 0) {
        await this.selectPlaylist(this.playlists[0].id);
      }
    } catch (err) {
      console.error("Failed to initialize PlaylistsStore:", err);
    }
  }

  async refreshPlaylists() {
    this.playlists = await invoke("get_playlists");
  }

  async selectPlaylist(id: number) {
    this.activePlaylistId = id;
    this.activePlaylistTracks = await invoke("get_playlist_tracks", { playlistId: id });
    try {
      await invoke("set_app_setting", { key: "active_playlist_id", value: id.toString() });
    } catch (err) {
      console.error("Failed to save active playlist settings:", err);
    }
  }

  async createPlaylist(name: string) {
    const playlist: Playlist = await invoke("create_playlist", { name });
    await this.refreshPlaylists();
    await this.selectPlaylist(playlist.id);
  }

  async deletePlaylist(id: number) {
    await invoke("delete_playlist", { id });
    await this.refreshPlaylists();
    if (this.activePlaylistId === id) {
      if (this.playlists.length > 0) {
        await this.selectPlaylist(this.playlists[0].id);
      } else {
        this.activePlaylistId = null;
        this.activePlaylistTracks = [];
      }
    }
  }

  async renamePlaylist(id: number, name: string) {
    await invoke("rename_playlist", { id, name });
    await this.refreshPlaylists();
  }

  async addSongsToPlaylist(playlistId: number, songIds: number[]) {
    await invoke("add_to_playlist", { playlistId, songIds });
    if (this.activePlaylistId === playlistId) {
      await this.selectPlaylist(playlistId);
    }
    await this.refreshPlaylists(); // update track counts
  }

  async removeItemsFromPlaylist(playlistId: number, uuids: string[]) {
    await invoke("remove_from_playlist", { playlistId, uuids });
    if (this.activePlaylistId === playlistId) {
      await this.selectPlaylist(playlistId);
    }
    await this.refreshPlaylists(); // update track counts
  }

  async reorderItem(playlistId: number, fromIndex: number, toIndex: number) {
    if (this.activePlaylistId === playlistId && fromIndex >= 0 && toIndex >= 0 && fromIndex < this.activePlaylistTracks.length && toIndex < this.activePlaylistTracks.length) {
      const updated = [...this.activePlaylistTracks];
      const [moved] = updated.splice(fromIndex, 1);
      updated.splice(toIndex, 0, moved);
      this.activePlaylistTracks = updated;
    }
    await invoke("reorder_playlist_item", { playlistId, from: fromIndex, to: toIndex });
    if (this.activePlaylistId === playlistId) {
      await this.selectPlaylist(playlistId);
    }
  }

  async clearPlaylist(playlistId: number) {
    await invoke("clear_playlist", { playlistId });
    if (this.activePlaylistId === playlistId) {
      this.activePlaylistTracks = [];
    }
    await this.refreshPlaylists(); // update track counts
  }

  async undo() {
    await invoke("undo_playlist");
    if (this.activePlaylistId !== null) {
      await this.selectPlaylist(this.activePlaylistId);
    }
    await this.refreshPlaylists();
  }

  async redo() {
    await invoke("redo_playlist");
    if (this.activePlaylistId !== null) {
      await this.selectPlaylist(this.activePlaylistId);
    }
    await this.refreshPlaylists();
  }
}

export const playlistsStore = new PlaylistsStore();
