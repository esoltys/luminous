import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Playlist, PlaylistItem, Song } from "../types";
import { applySongStats, type SongStatsPayload } from "../utils/stats";

class PlaylistsStore {
  playlists = $state<Playlist[]>([]);
  activePlaylistId = $state<number | null>(null);
  activePlaylistTracks = $state<PlaylistItem[]>([]);

  /** Live song counts for the virtual (non-materialized) auto-playlists. */
  favouritesCount = $state(0);
  recentlyAddedCount = $state(0);

  /** Count of auto-playlists that currently have at least one song — used for
   * the sidebar's Auto badge, kept in sync with the Auto grid's own filtering. */
  get visibleAutoPlaylistCount(): number {
    const genreCount = this.playlists.filter((p) => p.dynamic_enabled && p.track_count > 0).length;
    return genreCount + (this.favouritesCount > 0 ? 1 : 0) + (this.recentlyAddedCount > 0 ? 1 : 0);
  }

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
        this.refreshAutoPlaylistCounts();
      });

      await this.refreshPlaylists();
      this.refreshAutoPlaylistCounts();
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

  async refreshAutoPlaylistCounts() {
    try {
      const [favourites, recentlyAdded] = await Promise.all([
        invoke<Song[]>("get_favourite_songs"),
        invoke<Song[]>("get_recently_added_songs", { limit: 50 }),
      ]);
      this.favouritesCount = Array.isArray(favourites) ? favourites.length : 0;
      this.recentlyAddedCount = Array.isArray(recentlyAdded) ? recentlyAdded.length : 0;
      await this.refreshPlaylists();
    } catch (err) {
      console.error("Failed to refresh auto-playlist counts:", err);
    }
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

  async reorderItemsBatch(playlistId: number, fromIndices: number[], toIndex: number) {
    if (fromIndices.length === 0) return;
    if (fromIndices.length === 1) {
      return this.reorderItem(playlistId, fromIndices[0], toIndex);
    }
    await invoke("reorder_playlist_items", { playlistId, fromIndices, toIndex });
    if (this.activePlaylistId === playlistId) {
      await this.selectPlaylist(playlistId);
    }
  }

  async deduplicatePlaylist(playlistId: number) {
    const tracks = this.activePlaylistId === playlistId
      ? this.activePlaylistTracks
      : await invoke<PlaylistItem[]>("get_playlist_tracks", { playlistId });
    
    const seen = new Set<string>();
    const duplicateUuids: string[] = [];

    for (const item of tracks) {
      let key = "";
      if (item.song?.id) {
        key = `song-${item.song.id}`;
      } else if (item.song?.title && item.song?.artist) {
        key = `meta-${item.song.title.toLowerCase().trim()}-${item.song.artist.toLowerCase().trim()}`;
      } else if (item.url) {
        key = `url-${item.url}`;
      } else {
        key = `uuid-${item.uuid}`;
      }

      if (seen.has(key)) {
        duplicateUuids.push(item.uuid);
      } else {
        seen.add(key);
      }
    }

    if (duplicateUuids.length > 0) {
      await this.removeItemsFromPlaylist(playlistId, duplicateUuids);
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

  async importPlaylist(filePath: string) {
    const playlist: Playlist = await invoke("import_playlist", { filePath });
    await this.refreshPlaylists();
    await this.selectPlaylist(playlist.id);
  }

  async exportPlaylist(playlistId: number, exportPath: string, relative: boolean = true) {
    await invoke("export_playlist", { playlistId, exportPath, relative });
  }

  /**
   * Toggle the Auto-Play flag on a dynamic/auto playlist.
   * When enabled, playback will keep appending the next batch of matching
   * songs as the queue approaches the end of the current batch (#26).
   */
  async setPlaylistAutoPlay(playlistId: number, autoPlay: boolean) {
    await invoke("set_playlist_auto_play", { playlistId, autoPlay });
    await this.refreshPlaylists();
  }
}

export const playlistsStore = new PlaylistsStore();
