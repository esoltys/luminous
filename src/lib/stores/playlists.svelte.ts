import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Playlist, PlaylistItem, QueuePopulationMode, Song } from "../types";
import { applySongStats, type SongStatsPayload } from "../utils/stats";
import { toastStore } from "./toast.svelte";
import { i18n } from "./i18n.svelte";

/** Marker substring from the backend's reserved-name rejection (playlist.rs,
 * is_reserved_playlist_name) — always in English regardless of locale, since
 * it's matched here rather than shown directly to the user. */
const RESERVED_PLAYLIST_NAME_MARKER = "is reserved for the app's built-in Queue playlist";

function isReservedPlaylistNameError(err: unknown): boolean {
  return String(err).includes(RESERVED_PLAYLIST_NAME_MARKER);
}

class PlaylistsStore {
  playlists = $state<Playlist[]>([]);
  activePlaylistId = $state<number | null>(null);
  activePlaylistTracks = $state<PlaylistItem[]>([]);

  /** The playlist explicitly pinned via "Make Active" as the quick-add target
   * used across the app ("Add to {name}"). Distinct from `activePlaylistId`
   * (which just tracks whichever playlist is currently loaded/viewed) so
   * that merely opening a playlist to look at it never changes this. Only
   * `pinPlaylist()` (wired to the "Make Active" button) mutates it. */
  pinnedPlaylistId = $state<number | null>(null);

  /** Live song counts for the virtual (non-materialized) auto-playlists. */
  favouritesCount = $state(0);
  recentlyAddedCount = $state(0);
  historyCount = $state(0);

  /** Count of auto-playlists that currently have at least one song — used for
   * the sidebar's Auto badge, kept in sync with the Auto grid's own filtering. */
  visibleAutoPlaylistCount = $derived.by(() => {
    const genreCount = this.playlists.filter((p) => p.dynamic_enabled && p.track_count > 0).length;
    return (
      genreCount +
      (this.favouritesCount > 0 ? 1 : 0) +
      (this.recentlyAddedCount > 0 ? 1 : 0) +
      (this.historyCount > 0 ? 1 : 0)
    );
  });

  /** The special pinned Queue playlist, always present and never deletable. */
  queuePlaylist = $derived.by((): Playlist | null => {
    return (
      this.playlists.find((p) => !p.dynamic_enabled && p.name?.toLowerCase() === "queue") ?? null
    );
  });

  queueVersion = $state(0);

  notifyQueueChanged() {
    this.queueVersion++;
  }

  queueTrackCount = $derived.by((): number => {
    // Explicit signal dependency for Svelte 5 reactivity
    this.queueVersion;
    const q = this.playlists.find((p) => !p.dynamic_enabled && p.name?.toLowerCase() === "queue");
    if (q === undefined) return this.activePlaylistTracks.length ?? 0;
    if (this.activePlaylistId !== null && this.activePlaylistId === q.id && this.activePlaylistTracks.length > (q.track_count ?? 0)) {
      return this.activePlaylistTracks.length;
    }
    return q.track_count ?? 0;
  });

  /** The "Active" playlist id: whatever's explicitly pinned via "Make Active",
   * falling back to the Queue playlist when nothing has been pinned yet. */
  effectivePinnedPlaylistId = $derived.by((): number | null => {
    return this.pinnedPlaylistId ?? this.queuePlaylist?.id ?? null;
  });

  /** The pinned/"Active" playlist ONLY if it is a custom playlist (not an auto/dynamic playlist). */
  activeCustomPlaylist = $derived.by((): Playlist | null => {
    if (this.effectivePinnedPlaylistId === null) return null;
    const pl = this.playlists.find((p) => p.id === this.effectivePinnedPlaylistId);
    return pl && !pl.dynamic_enabled ? pl : null;
  });

  constructor() {
    this.init();
  }

  async ensureQueuePlaylist(): Promise<Playlist | null> {
    if (!Array.isArray(this.playlists)) return null;
    let queuePl = this.playlists.find((p) => p && !p.dynamic_enabled && p.name && p.name.toLowerCase() === "queue");
    if (!queuePl) {
      try {
        const created: Playlist = await invoke("create_playlist", { name: "Queue" });
        await this.refreshPlaylists();
        if (Array.isArray(this.playlists)) {
          queuePl = this.playlists.find((p) => p && p.id === created?.id) || created;
        }
      } catch (err) {
        console.error("Failed to auto-create Queue playlist:", err);
      }
    }
    return queuePl || null;
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
      const queuePl = await this.ensureQueuePlaylist();

      const settings = await invoke<Record<string, string>>("get_all_app_settings");
      if (settings && settings.pinned_playlist_id) {
        const pinnedId = parseInt(settings.pinned_playlist_id, 10);
        if (!isNaN(pinnedId) && this.playlists.some((p) => p.id === pinnedId)) {
          this.pinnedPlaylistId = pinnedId;
        }
      }

      if (settings && settings.active_playlist_id) {
        const plId = parseInt(settings.active_playlist_id, 10);
        if (!isNaN(plId) && this.playlists.some((p) => p.id === plId)) {
          await this.selectPlaylist(plId);
          return;
        }
      }
      if (queuePl) {
        await this.selectPlaylist(queuePl.id);
      } else if (this.playlists.length > 0) {
        await this.selectPlaylist(this.playlists[0].id);
      }
    } catch (err) {
      console.error("Failed to initialize PlaylistsStore:", err);
    }
  }

  async refreshPlaylists() {
    this.playlists = await invoke("get_playlists");
    this.notifyQueueChanged();
  }

  async refreshAutoPlaylistCounts() {
    try {
      const [favourites, recentlyAdded, history] = await Promise.all([
        invoke<Song[]>("get_favourite_songs"),
        invoke<Song[]>("get_recently_added_songs", { limit: 50 }),
        invoke<Song[]>("get_recently_played_songs", { limit: 100 }),
      ]);
      this.favouritesCount = Array.isArray(favourites) ? favourites.length : 0;
      this.recentlyAddedCount = Array.isArray(recentlyAdded) ? recentlyAdded.length : 0;
      this.historyCount = Array.isArray(history) ? history.length : 0;
      await this.refreshPlaylists();
    } catch (err) {
      console.error("Failed to refresh auto-playlist counts:", err);
    }
  }

  async selectPlaylist(id: number) {
    this.activePlaylistId = id;
    this.activePlaylistTracks = await invoke("get_playlist_tracks", { playlistId: id });
    this.notifyQueueChanged();
    try {
      await invoke("set_app_setting", { key: "active_playlist_id", value: id.toString() });
    } catch (err) {
      console.error("Failed to save active playlist settings:", err);
    }
  }

  /** Explicitly pins `id` as the "Active" quick-add target — only called from
   * the "Make Active" button. Never call this as a side effect of merely
   * viewing/opening a playlist. */
  async pinPlaylist(id: number) {
    this.pinnedPlaylistId = id;
    try {
      await invoke("set_app_setting", { key: "pinned_playlist_id", value: id.toString() });
    } catch (err) {
      console.error("Failed to save pinned playlist setting:", err);
    }
  }

  async createPlaylist(name: string): Promise<Playlist> {
    let playlist: Playlist;
    try {
      playlist = await invoke("create_playlist", { name });
    } catch (err) {
      if (isReservedPlaylistNameError(err)) {
        toastStore.show(i18n.t("playlists.reservedPlaylistName", { name: name.trim() }), "error");
      }
      throw err;
    }
    await this.refreshPlaylists();
    await this.selectPlaylist(playlist.id);
    await this.pinPlaylist(playlist.id);
    toastStore.show(
      i18n.t("celebrations.playlistCreated", { name }, `Playlist "${name}" created`),
      "success"
    );
    return playlist;
  }

  async updatePlaylistSpec(
    id: number,
    spec: string,
    autoPlay: boolean = false,
    populationMode: QueuePopulationMode = "all"
  ) {
    // Mode is persisted before the spec so that the spec-triggered
    // (re)population below already uses it.
    await invoke("set_playlist_population_mode", { playlistId: id, mode: populationMode });
    await invoke("set_playlist_dynamic_spec", { playlistId: id, spec });
    await invoke("set_playlist_auto_play", { playlistId: id, autoPlay });
    await this.refreshPlaylists();
    if (this.activePlaylistId === id) {
      await this.selectPlaylist(id);
    }
  }

  async deletePlaylist(id: number) {
    if (Array.isArray(this.playlists)) {
      const target = this.playlists.find((p) => p && p.id === id);
      if (target && !target.dynamic_enabled && target.name && target.name.toLowerCase() === "queue") {
        console.warn("Cannot delete special Queue playlist");
        return;
      }
    }
    await invoke("delete_playlist", { id });
    await this.refreshPlaylists();
    if (this.activePlaylistId === id) {
      if (Array.isArray(this.playlists) && this.playlists.length > 0) {
        await this.selectPlaylist(this.playlists[0].id);
      } else {
        this.activePlaylistId = null;
        this.activePlaylistTracks = [];
      }
    }
    if (this.pinnedPlaylistId === id) {
      this.pinnedPlaylistId = null;
      try {
        await invoke("set_app_setting", { key: "pinned_playlist_id", value: "" });
      } catch (err) {
        console.error("Failed to clear pinned playlist setting:", err);
      }
    }
  }

  async renamePlaylist(id: number, name: string) {
    try {
      await invoke("rename_playlist", { id, name });
    } catch (err) {
      if (isReservedPlaylistNameError(err)) {
        toastStore.show(i18n.t("playlists.reservedPlaylistName", { name: name.trim() }), "error");
      }
      throw err;
    }
    await this.refreshPlaylists();
  }

  async replaceQueueTracks(songIds: number[]) {
    const queuePl = await this.ensureQueuePlaylist();
    if (!queuePl) return;
    try {
      const existingItems: PlaylistItem[] = await invoke("get_playlist_tracks", { playlistId: queuePl.id });
      if (existingItems.length > 0) {
        const uuids = existingItems.map((i) => i.uuid);
        await invoke("remove_from_playlist", { playlistId: queuePl.id, uuids });
      }
      if (songIds.length > 0) {
        await invoke("add_to_playlist", { playlistId: queuePl.id, songIds });
      }
      await this.selectPlaylist(queuePl.id);
      await this.refreshPlaylists();
    } catch (err) {
      console.error("Failed to sync Queue playlist tracks:", err);
    }
  }

  /** Removes every track ahead of `uuid` (in current DB order) from
   * `playlistId`. Takes the target playlist id explicitly rather than
   * re-resolving "the Queue" via `ensureQueuePlaylist()` — that lookup can
   * disagree with whichever playlist the caller actually means (e.g. is
   * already viewing), and always resolves `uuid`'s position from a fresh
   * `get_playlist_tracks` fetch rather than trusting a caller-supplied
   * index, since the Queue can be trimmed in the background (see
   * `syncQueueTrackPosition`) between when a position is read on the
   * frontend and when a trim actually runs — an index captured too early
   * would otherwise cut the wrong rows. */
  async trimQueueBeforeUuid(playlistId: number, uuid: string) {
    try {
      const existingItems: PlaylistItem[] = await invoke("get_playlist_tracks", { playlistId });
      const index = existingItems.findIndex((i) => i.uuid === uuid);
      if (index > 0) {
        const uuidsToRemove = existingItems.slice(0, index).map((i) => i.uuid);
        await invoke("remove_from_playlist", { playlistId, uuids: uuidsToRemove });
        await invoke("remove_songs_from_player_playlist", { uuids: uuidsToRemove });
        if (this.activePlaylistId === playlistId) {
          await this.selectPlaylist(playlistId);
        }
        await this.refreshPlaylists();
      }
    } catch (err) {
      console.error("Failed to trim Queue tracks before uuid:", err);
    }
  }

  async addSongsToPlaylist(playlistId: number, songIds: number[]) {
    await invoke("add_to_playlist", { playlistId, songIds });
    await this.refreshPlaylists(); // update track counts FIRST
    const qPl = this.queuePlaylist;
    if (this.activePlaylistId === playlistId || (qPl && playlistId === qPl.id)) {
      await this.selectPlaylist(playlistId);
    }
  }

  async removeItemsFromPlaylist(playlistId: number, uuids: string[]) {
    await invoke("remove_from_playlist", { playlistId, uuids });
    // Keep the live playback queue in sync — a no-op if these uuids aren't
    // part of the currently loaded queue (see #262).
    await invoke("remove_songs_from_player_playlist", { uuids });
    await this.refreshPlaylists(); // update track counts FIRST
    const qPl = this.queuePlaylist;
    if (this.activePlaylistId === playlistId || (qPl && playlistId === qPl.id)) {
      await this.selectPlaylist(playlistId);
    }
  }

  async reorderItemByUuid(playlistId: number, sourceUuid: string, targetUuid: string) {
    if (sourceUuid === targetUuid) return;
    const fromIdx = this.activePlaylistTracks.findIndex((t) => t.uuid === sourceUuid);
    const toIdx = this.activePlaylistTracks.findIndex((t) => t.uuid === targetUuid);
    if (this.activePlaylistId === playlistId && fromIdx !== -1 && toIdx !== -1) {
      const updated = [...this.activePlaylistTracks];
      const [moved] = updated.splice(fromIdx, 1);
      updated.splice(toIdx, 0, moved);
      this.activePlaylistTracks = updated;
    }
    await invoke("reorder_playlist_item_by_uuid", { playlistId, sourceUuid, targetUuid });
    const queuePl = this.queuePlaylist;
    if (queuePl && playlistId === queuePl.id) {
      await invoke("reorder_player_item_by_uuid", { sourceUuid, targetUuid });
    }
    if (this.activePlaylistId === playlistId) {
      await this.selectPlaylist(playlistId);
    }
  }

  async reorderItem(playlistId: number, fromIndex: number, toIndex: number) {
    if (this.activePlaylistId === playlistId && fromIndex >= 0 && toIndex >= 0 && fromIndex < this.activePlaylistTracks.length && toIndex < this.activePlaylistTracks.length) {
      const updated = [...this.activePlaylistTracks];
      const [moved] = updated.splice(fromIndex, 1);
      updated.splice(toIndex, 0, moved);
      this.activePlaylistTracks = updated;
    }
    await invoke("reorder_playlist_item", { playlistId, from: fromIndex, to: toIndex });
    const queuePl = this.queuePlaylist;
    if (queuePl && playlistId === queuePl.id) {
      await invoke("reorder_player_playlist_items", { fromIndex, toIndex });
    }
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
    const queuePl = this.queuePlaylist;
    if (queuePl && playlistId === queuePl.id) {
      await invoke("clear_player_playlist");
    }
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

  /**
   * Change a playlist's queue population bias (All/Favourites/Familiar/
   * Discover/Deep Cuts — #120). The backend persists the mode and
   * immediately regenerates the playlist's tracks, so the change takes
   * effect right away rather than waiting for the next refresh.
   */
  async setPlaylistPopulationMode(playlistId: number, mode: QueuePopulationMode) {
    await invoke("set_playlist_population_mode", { playlistId, mode });
    await this.refreshPlaylists();
    if (this.activePlaylistId === playlistId) {
      await this.selectPlaylist(playlistId);
    }
  }

  /**
   * Force-refresh an auto-playlist with a fresh set of matching songs from the library.
   */
  async refreshAutoPlaylist(playlistId: number) {
    await invoke("refresh_auto_playlist", { playlistId });
    await this.refreshPlaylists();
    if (this.activePlaylistId === playlistId) {
      await this.selectPlaylist(playlistId);
    }
  }
}

export const playlistsStore = new PlaylistsStore();
