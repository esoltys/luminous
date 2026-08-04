import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type { PlaybackState, Playlist, PlaylistItem, Song, ShuffleMode, RepeatMode, PlayState, LoudnessGainSource, PlayContext } from "../types";
import { applySongStats, type SongStatsPayload } from "../utils/stats";
import { themeStore } from "./theme.svelte";
import { toastStore } from "./toast.svelte";
import { playlistsStore } from "./playlists.svelte";
import { i18n } from "./i18n.svelte";

/** Minimum remaining tracks before the auto-refill is triggered (#26). */
const AUTO_PLAY_REFILL_THRESHOLD = 3;


export class PlayerStore {
  // Reactive state using Svelte 5 Runes
  state = $state<PlayState>("stopped");
  currentSong = $state<Song | undefined>(undefined);
  playlistId = $state<number | undefined>(undefined);
  playlistItemUuid = $state<string | undefined>(undefined);
  positionNanosec = $state<number>(0);
  volume = $state<number>(1.0);
  shuffleMode = $state<ShuffleMode>("off");
  repeatMode = $state<RepeatMode>("off");
  stopAfterCurrent = $state<boolean>(false);
  loudnessSource = $state<LoudnessGainSource>("disabled");
  loudnessGainDb = $state<number | undefined>(undefined);
  /** Tracks remaining after the current one; populated from PlaybackState (#26). */
  remainingPlaylistItems = $state<number>(0);
  /** Set of playlist IDs whose library auto-refill pool has been exhausted. */
  exhaustedPlaylistIds = $state<number[]>([]);

  /** Celebration: queue just finished naturally (#182, Milestone tier). */
  queueJustCompleted = $state<boolean>(false);
  /** The name of the active playlist, album, or source context being played. */
  activeContextName = $state<string | undefined>(undefined);
  /** Previous playback state for detecting playing→stopped transitions. */
  private _previousState: PlayState = "stopped";

  /** Titles pending a "couldn't play" toast — batched so a run of consecutive
   *  unavailable tracks (e.g. a whole disconnected drive) shows one summary
   *  toast instead of a notification per failed track. */
  private _playbackErrorBatch: string[] = [];
  private _playbackErrorTimer: ReturnType<typeof setTimeout> | null = null;

  isAutoPlayExhausted(playlistId: number): boolean {
    return this.exhaustedPlaylistIds.includes(playlistId);
  }

  clearExhausted(playlistId: number) {
    this.exhaustedPlaylistIds = this.exhaustedPlaylistIds.filter((id) => id !== playlistId);
  }

  /** Prevents concurrent refill invocations for the same playlist. */
  private _refillInFlight = false;

  constructor() {
    this.init();
  }

  private async init() {
    try {
      // Get initial playback state from backend
      const initialState: PlaybackState = await invoke("get_playback_state");
      this.updateState(initialState);
      themeStore.updateArtworkColors(this.currentSong);

      // Listen for position changes (emitted every ~250ms or on seek)
      await listen<{ position_nanosec: number }>("playback-position", (event) => {
        this.positionNanosec = event.payload.position_nanosec;
      });

      // Listen for playback state changes
      await listen<PlaybackState>("playback-state", async (event) => {
        const oldSongId = this.currentSong?.id;
        const oldSongAlbum = this.currentSong?.album;
        const wasPlaying = this._previousState === "playing";
        const prevShuffle = this.shuffleMode;
        this.updateState(event.payload);
        this._previousState = this.state;
        if (prevShuffle !== this.shuffleMode && playlistsStore.activePlaylistId !== null) {
          await playlistsStore.selectPlaylist(playlistsStore.activePlaylistId);
        }
        if (this.currentSong?.id !== oldSongId) {
          themeStore.updateArtworkColors(this.currentSong);
          await this.syncQueueTrackPosition();
        }

        // Queue completion celebration (#182, Milestone tier): fires when
        // playback stops naturally after the last track with nothing left.
        if (wasPlaying && this.state === "stopped" && this.remainingPlaylistItems === 0 && !this.currentSong) {
          this.queueJustCompleted = true;

          const toastText = i18n.t("celebrations.queueComplete", {}, "Your Queue is done");
          toastStore.show(toastText, "milestone");
          setTimeout(() => { this.queueJustCompleted = false; }, 650);
        }

        // Auto-Play refill: checked on every state change so we react when
        // remaining drops below threshold after each track advance (#26).
        await this.maybeRefillAutoPlaylist();
      });

      // Listen for track changes
      await listen<{ song: Song | null }>("track-changed", async (event) => {
        this.currentSong = event.payload.song || undefined;
        themeStore.updateArtworkColors(this.currentSong);
        await this.syncQueueTrackPosition();
      });

      // A song couldn't be opened/decoded (e.g. its file just vanished —
      // watched drive disconnected). The backend already skips past it; this
      // just tells the user why playback jumped. Batched (see
      // _playbackErrorBatch) because a disconnected drive mid-playlist can
      // fail several consecutive tracks in a row before the backend's
      // circuit breaker stops it — one toast per failure would be a
      // notification avalanche.
      await listen<{ songId: number; title: string | null; path: string | null }>(
        "playback-error",
        (event) => {
          this._playbackErrorBatch.push(event.payload.title || i18n.t("collection.unknownSong"));
          if (this._playbackErrorTimer) clearTimeout(this._playbackErrorTimer);
          this._playbackErrorTimer = setTimeout(() => this.flushPlaybackErrorToast(), 400);
        }
      );

      // Keep the current song's stats in sync when they change elsewhere
      // (rating edits in list views, scrobble-point playcount bumps).
      await listen<SongStatsPayload>("song-stats-changed", (event) => {
        if (this.currentSong && this.currentSong.id === event.payload.song_id) {
          applySongStats(this.currentSong, event.payload);
        }
      });

      // A file association (or `luminous <file>`) launched a second instance
      // while this one was already running; the backend forwards the paths
      // here instead of spawning a separate window.
      await listen<string[]>("open-file-request", (event) => {
        this.openAndPlay(event.payload);
      });

      // Check for startup file argument
      const startupFile = await invoke<string | null>("get_startup_file");
      if (startupFile) {
        await this.openAndPlay([startupFile]);
      }
    } catch (err) {
      console.error("Failed to initialize PlayerStore:", err);
    }
  }

  private flushPlaybackErrorToast() {
    const titles = this._playbackErrorBatch;
    this._playbackErrorBatch = [];
    this._playbackErrorTimer = null;
    if (titles.length === 0) return;

    if (titles.length === 1) {
      toastStore.show(
        i18n.t("playerBar.trackSkippedToast", { title: titles[0] }, `Couldn't play "${titles[0]}" — file not found. Skipped.`),
        "error"
      );
    } else {
      toastStore.show(
        i18n.t("playerBar.tracksSkippedToast", { count: titles.length }, `Skipped ${titles.length} unavailable tracks.`),
        "error"
      );
    }
  }

  private updateState(state: PlaybackState) {
    if (!state) return;
    this.state = state.state;
    this.currentSong = state.current_song;
    this.playlistId = state.playlist_id;
    this.playlistItemUuid = state.playlist_item_uuid;
    this.positionNanosec = state.position_nanosec;
    this.volume = state.volume;
    this.shuffleMode = state.shuffle_mode;
    this.repeatMode = state.repeat_mode;
    this.stopAfterCurrent = state.stop_after_current;
    this.loudnessSource = state.loudness_source;
    this.loudnessGainDb = state.loudness_gain_db;
    this.remainingPlaylistItems = state.remaining_playlist_items ?? 0;
  }

  /**
   * Auto-Play refill (#26): when a dynamic playlist has auto_play enabled and
   * the remaining track count drops below the threshold, fetch the next batch
   * and append it to both the DB playlist and the live player queue.
   */
  private async maybeRefillAutoPlaylist() {
    const pid = this.playlistId;
    if (!pid || pid === 0) return;
    if (this.remainingPlaylistItems >= AUTO_PLAY_REFILL_THRESHOLD) return;
    if (this._refillInFlight) return;

    // Look up this playlist to check dynamic_enabled + auto_play
    try {
      const playlists = await invoke<Playlist[]>("get_playlists");
      const pl = playlists.find((p) => p.id === pid);
      if (!pl?.dynamic_enabled || !pl?.auto_play) return;

      this._refillInFlight = true;
      const newSongs = await invoke<Song[]>("refill_auto_playlist", { playlistId: pid });
      if (newSongs.length > 0) {
        const songIds = newSongs.map((s) => s.id);
        await invoke("append_songs_to_player_playlist", { songIds });
        // The backend emits a playback-state event after appending, but
        // refresh explicitly too so remainingPlaylistItems is guaranteed
        // to be current before _refillInFlight resets — otherwise a stale
        // count keeps re-triggering this refill in a runaway loop (#194).
        await this.refreshPlaybackState();
        const { playlistsStore } = await import("./playlists.svelte");
        if (playlistsStore.activePlaylistId === pid) {
          await playlistsStore.selectPlaylist(pid);
        }
        await playlistsStore.refreshPlaylists();
      } else {
        // No new songs returned: all matching tracks in library have been added
        if (!this.exhaustedPlaylistIds.includes(pid)) {
          this.exhaustedPlaylistIds = [...this.exhaustedPlaylistIds, pid];
        }
      }
    } catch (err) {
      console.error("[PlayerStore] Auto-Play refill failed:", err);
    } finally {
      this._refillInFlight = false;
    }
  }

  /** Force a refresh of the playback state from the backend (e.g., after tags are edited) */
  async refreshPlaybackState() {
    try {
      const state = await invoke<PlaybackState>("get_playback_state");
      this.updateState(state);
      if (this.currentSong) {
        themeStore.updateArtworkColors(this.currentSong);
      }
    } catch (err) {
      console.error("[PlayerStore] Failed to refresh playback state:", err);
    }
  }

  // Playback Control Actions
  async playSong(songId: number) {
    await invoke("play_song", { songId });
  }

  async openAndPlay(paths: string[]) {
    await invoke("open_and_play", { paths });
  }

  async openFileDialog() {
    try {
      const selected = await open({
        multiple: true,
        directory: false,
        title: i18n.t('topNav.openFilesTitle', {}, "Open Audio Files or Playlists"),
        filters: [
          {
            name: "Supported Files",
            extensions: ["mp3", "flac", "ogg", "opus", "m4a", "aac", "alac", "wav", "aiff", "aif", "wv", "mpc", "ape", "tta", "dsf", "dff", "asf", "wma", "m4b", "m3u", "m3u8", "pls", "xspf"]
          },
          {
            name: "Audio Files",
            extensions: ["mp3", "flac", "ogg", "opus", "m4a", "aac", "alac", "wav", "aiff", "aif", "wv", "mpc", "ape", "tta", "dsf", "dff", "asf", "wma", "m4b"]
          },
          {
            name: "Playlists",
            extensions: ["m3u", "m3u8", "pls", "xspf"]
          }
        ]
      });

      if (selected) {
        const paths = Array.isArray(selected) ? selected : [selected];
        if (paths.length > 0) {
          await this.openAndPlay(paths);
        }
      }
    } catch (err) {
      console.error("Failed to open files/playlists:", err);
    }
  }

  async playSongs(songIds: number[], startIndex: number, playlistId?: number, context?: PlayContext, contextName?: string) {
    if (contextName) {
      this.activeContextName = contextName;
    } else if (context?.type === "album" && context.album) {
      this.activeContextName = context.album;
    } else if (context?.type === "playlist" && context.playlistId) {
      const pl = playlistsStore.playlists.find((p) => p.id === context.playlistId);
      if (pl) this.activeContextName = pl.name;
    } else if (playlistId) {
      const pl = playlistsStore.playlists.find((p) => p.id === playlistId);
      if (pl) this.activeContextName = pl.name;
    } else {
      this.activeContextName = undefined;
    }
    await playlistsStore.replaceQueueTracks(songIds);
    await invoke("play_songs", { songIds, startIndex, playlistId: playlistId ?? null, context: context ?? null });
  }

  private async syncQueueTrackPosition() {
    const queuePl = await playlistsStore.ensureQueuePlaylist();
    if (!queuePl || !this.currentSong) return;
    if (this.playlistId === queuePl.id || this.activeContextName === "Queue") {
      const tracks = playlistsStore.activePlaylistId === queuePl.id
        ? playlistsStore.activePlaylistTracks
        : await invoke<PlaylistItem[]>("get_playlist_tracks", { playlistId: queuePl.id });
      if (tracks.length > 0) {
        const activeIndex = tracks.findIndex((t: PlaylistItem) => t.song?.id === this.currentSong?.id);
        if (activeIndex > 0) {
          await playlistsStore.trimQueueBeforeIndex(activeIndex);
        }
      }
    }
  }

  async playPlaylistItem(playlistId: number, itemIndex: number) {
    const pl = playlistsStore.playlists.find((p) => p.id === playlistId);
    if (pl) this.activeContextName = pl.name;
    const queuePl = await playlistsStore.ensureQueuePlaylist();
    if (queuePl && (playlistId === queuePl.id || pl?.name?.toLowerCase() === "queue")) {
      if (itemIndex > 0) {
        await playlistsStore.trimQueueBeforeIndex(itemIndex);
        await invoke("play_playlist_item", { playlistId: queuePl.id, itemIndex: 0 });
        return;
      }
    }
    await invoke("play_playlist_item", { playlistId, itemIndex });
  }

  async pause() {
    await invoke("pause");
  }

  async resume() {
    await invoke("resume");
  }

  async togglePlayPause() {
    if (this.state === "playing") {
      await this.pause();
    } else {
      await this.resume();
    }
  }

  async stop() {
    await invoke("stop");
  }

  async next() {
    await invoke("next_track");
  }

  async previous() {
    await invoke("previous_track");
  }

  async seek(positionNs: number) {
    const roundedNs = Math.round(positionNs);
    this.positionNanosec = roundedNs;
    console.log("[PlayerStore] Seeking to nanoseconds (rounded):", roundedNs, "original float:", positionNs);
    await invoke("seek_to", { positionNanosec: roundedNs });
  }

  async seekRelative(deltaNs: number) {
    const durationNs = this.currentSong?.length_nanosec;
    const maxPositionNs = typeof durationNs === "number" ? durationNs : Number.POSITIVE_INFINITY;
    const nextPositionNs = Math.min(Math.max(this.positionNanosec + deltaNs, 0), maxPositionNs);
    await this.seek(nextPositionNs);
  }

  async setVolume(vol: number) {
    this.volume = vol;
    await invoke("set_volume", { volume: vol });
  }

  async adjustVolume(delta: number) {
    const nextVolume = Math.round(Math.min(Math.max(this.volume + delta, 0), 1) * 100) / 100;
    await this.setVolume(nextVolume);
  }

  async setShuffleMode(mode: ShuffleMode) {
    this.shuffleMode = mode;
    await invoke("set_shuffle_mode", { mode });
    if (playlistsStore.activePlaylistId !== null) {
      await playlistsStore.selectPlaylist(playlistsStore.activePlaylistId);
    }
  }

  async setRepeatMode(mode: RepeatMode) {
    this.repeatMode = mode;
    await invoke("set_repeat_mode", { mode });
  }

  /** Rate the current track (-1 clears; hearts map to 5.0 via SongRating). */
  async rateCurrent(rating: number) {
    if (!this.currentSong) return;
    this.currentSong.rating = await invoke<number>("set_song_rating", {
      songId: this.currentSong.id,
      rating,
    });
  }
}

export const playerStore = new PlayerStore();
