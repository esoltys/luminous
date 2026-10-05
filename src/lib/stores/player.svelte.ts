import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type { PlaybackState, Playlist, PlaylistItem, Song, ShuffleMode, RepeatMode, PlayState, LoudnessGainSource, PlayContext, AudioPipelineInfo } from "../types";
import { applySongStats, type SongStatsPayload } from "../utils/stats";
import { themeStore } from "./theme.svelte";
import { toastStore } from "./toast.svelte";
import { playlistsStore } from "./playlists.svelte";
import { windowLayoutStore } from "./windowLayout.svelte";
import { i18n } from "./i18n.svelte";
import { isRemotePath } from "../utils/remoteSource";
import { shuffleArray } from "../utils/shuffle";

export interface CompletedSession {
  contextName: string;
  isQueue: boolean;
  songIds: number[];
  trackCount: number;
  completedAt?: number;
}

export class PlayerStore {
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
  audioPipeline = $state<AudioPipelineInfo | null>(null);
  /** Tracks remaining after the current one; populated from PlaybackState.
   * Only read for the queue-completion celebration (#182). */
  remainingPlaylistItems = $state<number>(0);
  /** Auto Continue (#1235): the backend tops up the Queue with similar songs
   * as it nears its end, so the Queue never "finishes". */
  autoContinue = $state<boolean>(false);

  /** Celebration: queue just finished naturally (#182, Milestone tier). */
  queueJustCompleted = $state<boolean>(false);
  /** Session wrap state for finished queue/playlist (#1379). */
  completedSession = $state<CompletedSession | null>(null);
  private _lastSessionSongIds: number[] = [];
  private _playedSongIdsHistory: number[] = [];
  /** The name of the active playlist, album, or source context being played. */
  activeContextName = $state<string | undefined>(undefined);
  /** Title to display for the current song: "Nothing playing" when there is no
   *  song, "Unknown Song" when the song has no title tag (#1246). */
  get currentSongDisplayTitle(): string {
    if (!this.currentSong) return i18n.t("playerBar.notPlaying");
    return this.currentSong.title || i18n.t("collection.unknownSong");
  }
  /** Previous playback state for detecting playing→stopped transitions. */
  private _previousState: PlayState = "stopped";

  /** Titles pending a "couldn't play" toast — batched so a run of consecutive
   *  unavailable tracks (e.g. a whole disconnected drive) shows one summary
   *  toast instead of a notification per failed track. */
  private _playbackErrorBatch: { title: string; path: string | null; message: string }[] = [];
  private _playbackErrorTimer: ReturnType<typeof setTimeout> | null = null;

  constructor() {
    this.init();
  }

  private async init() {
    try {
      const initialState: PlaybackState = await invoke("get_playback_state");
      this.updateState(initialState);
      themeStore.updateArtworkColors(this.currentSong);

      // Listen for position changes (emitted every ~250ms or on seek)
      await listen<{ position_nanosec: number }>("playback-position", (event) => {
        this.positionNanosec = event.payload.position_nanosec;
      });

      await listen<PlaybackState>("playback-state", async (event) => {
        const oldSongId = this.currentSong?.id;
        const oldItemUuid = this.playlistItemUuid;
        const oldSongAlbum = this.currentSong?.album;
        const wasPlaying = this._previousState === "playing";
        const prevShuffle = this.shuffleMode;
        const oldPlaylistId = this.playlistId;
        const oldContextName = this.activeContextName;
        this.updateState(event.payload);
        this._previousState = this.state;
        if (prevShuffle !== this.shuffleMode && playlistsStore.activePlaylistId !== null) {
          await playlistsStore.selectPlaylist(playlistsStore.activePlaylistId);
        }
        if (this.currentSong?.id !== oldSongId || this.playlistItemUuid !== oldItemUuid) {
          themeStore.updateArtworkColors(this.currentSong);
        }
        if (this.currentSong?.id) {
          this.completedSession = null;
          if (!this._playedSongIdsHistory.includes(this.currentSong.id)) {
            this._playedSongIdsHistory.push(this.currentSong.id);
          }
        }

        // Queue completion celebration (#182, Milestone tier): fires when
        // playback stops naturally after the last track with nothing left.
        if (wasPlaying && this.state === "stopped" && this.remainingPlaylistItems === 0 && !this.currentSong) {
          this.queueJustCompleted = true;

          const playedPl =
            oldPlaylistId !== null && oldPlaylistId !== undefined
              ? playlistsStore.playlists.find((p) => p.id === oldPlaylistId)
              : undefined;
          const isQueue = playedPl ? playedPl.is_queue : (!oldContextName || oldContextName === "Queue");

          const sessionTracks =
            this._lastSessionSongIds.length > 0
              ? [...this._lastSessionSongIds]
              : [...this._playedSongIdsHistory];

          const contextName = oldContextName || (isQueue ? i18n.t("playerBar.queueTitle") : "");
          this.completedSession = {
            contextName,
            isQueue,
            songIds: sessionTracks,
            trackCount: sessionTracks.length,
            completedAt: Date.now(),
          };

          // With Auto Continue on the Queue has no end to celebrate — it only
          // stops here if nothing in the library could be added.
          if (!(isQueue && this.autoContinue)) {
            // When in miniplayer or immersive mode, suppress the floating milestone toast since
            // both views render their own dedicated Session Wrap card (#1379, #1380).
            if (!windowLayoutStore.isMiniplayer && !windowLayoutStore.effectiveImmersiveMode) {
              const toastText = isQueue
                ? i18n.t("celebrations.queueComplete", {}, "Your Queue is done")
                : i18n.t("celebrations.contextComplete", { name: oldContextName }, `${oldContextName} complete`);
              toastStore.show(toastText, "milestone");
            }
          }
          setTimeout(() => { this.queueJustCompleted = false; }, 650);

          if (isQueue) {
            const queuePl = playlistsStore.queuePlaylist;
            if (queuePl) {
              await playlistsStore.clearPlaylist(queuePl.id);
            } else {
              try {
                const reqQueue = await playlistsStore.requireQueue();
                if (reqQueue) {
                  await playlistsStore.clearPlaylist(reqQueue.id);
                }
              } catch (err) {
                console.error("Failed to resolve Queue to clear:", err);
              }
            }
          }
        }
      });

      await listen<{ song: Song | null; pipeline?: AudioPipelineInfo | null }>("track-changed", async (event) => {
        this.currentSong = event.payload.song || undefined;
        this.audioPipeline = event.payload.pipeline ?? null;
        themeStore.updateArtworkColors(this.currentSong);
        if (this.currentSong?.id) {
          this.completedSession = null;
          if (!this._playedSongIdsHistory.includes(this.currentSong.id)) {
            this._playedSongIdsHistory.push(this.currentSong.id);
          }
        }
      });

      await listen<AudioPipelineInfo | null>("audio-pipeline-changed", (event) => {
        this.audioPipeline = event.payload;
      });

      // A song couldn't be opened/decoded (e.g. its file just vanished —
      // watched drive disconnected). The backend already skips past it; this
      // just tells the user why playback jumped. Batched (see
      // _playbackErrorBatch) because a disconnected drive mid-playlist can
      // fail several consecutive tracks in a row before the backend's
      // circuit breaker stops it — one toast per failure would be a
      // notification avalanche.
      await listen<{
        songId: number;
        title: string | null;
        path: string | null;
        message: string;
      }>(
        "playback-error",
        (event) => {
          this._playbackErrorBatch.push({
            title: event.payload.title || i18n.t("collection.unknownSong"),
            path: event.payload.path,
            message: event.payload.message,
          });
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

      const startupFile = await invoke<string | null>("get_startup_file");
      if (startupFile) {
        await this.openAndPlay([startupFile]);
      }

      const initialPipeline = await invoke<AudioPipelineInfo | null>("get_audio_pipeline_info").catch(() => null);
      if (initialPipeline) {
        this.audioPipeline = initialPipeline;
      }
    } catch (err) {
      console.error("Failed to initialize PlayerStore:", err);
    }
  }

  private flushPlaybackErrorToast() {
    const failures = this._playbackErrorBatch;
    this._playbackErrorBatch = [];
    this._playbackErrorTimer = null;
    if (failures.length === 0) return;

    if (failures.length === 1) {
      const { title, path, message } = failures[0];
      // A remote song (WebDAV/OpenSubsonic, #916) fails for reasons other than
      // a missing file — bad credentials, server down — so surface the
      // backend's reason instead of "file not found".
      const reason = message?.trim().replace(/[.\s]+$/, "");
      if (isRemotePath(path) && reason) {
        toastStore.show(
          i18n.t("playerBar.trackSkippedRemoteToast", { title, message: reason }, `Couldn't play "${title}" — ${reason}. Skipped.`),
          "error"
        );
      } else {
        toastStore.show(
          i18n.t("playerBar.trackSkippedToast", { title }, `Couldn't play "${title}" — file not found. Skipped.`),
          "error"
        );
      }
    } else {
      toastStore.show(
        i18n.t("playerBar.tracksSkippedToast", { count: failures.length }, `Skipped ${failures.length} unavailable tracks.`),
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
    this.autoContinue = state.auto_continue ?? false;
    if (!state.current_song) {
      this.audioPipeline = null;
    } else if (this.audioPipeline) {
      this.audioPipeline.loudness_source = state.loudness_source;
      this.audioPipeline.loudness_gain_db = state.loudness_gain_db;
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
    this._lastSessionSongIds = [songId];
    this._playedSongIdsHistory = [songId];
    this.completedSession = null;
    await invoke("play_song", { songId });
    await playlistsStore.refreshPlaylists();
    const queuePl = await playlistsStore.requireQueue();
    this.activeContextName = "Queue";
    await playlistsStore.selectPlaylist(queuePl.id);
  }

  async openAndPlay(paths: string[]) {
    this.completedSession = null;
    const outcome = await invoke<{ played: number; skipped: number }>("open_and_play", { paths });
    if (outcome.played === 0) {
      toastStore.show(
        i18n.t("playerBar.openNothingPlayable", {}, "No supported audio files found to play."),
        "error"
      );
      return outcome;
    }
    if (outcome.skipped > 0) {
      toastStore.show(
        i18n.t("playerBar.tracksSkippedToast", { count: outcome.skipped }, `Skipped ${outcome.skipped} unavailable tracks.`),
        "error"
      );
    }
    await playlistsStore.refreshPlaylists();
    const queuePl = await playlistsStore.requireQueue();
    this.activeContextName = "Queue";
    await playlistsStore.selectPlaylist(queuePl.id);
    return outcome;
  }

  /** Drag-and-dropped (or otherwise opened) paths, appended to the end of the
   * Queue instead of replacing it — the "hold Shift to append" counterpart to
   * `openAndPlay`. Playback is left untouched. */
  async addPathsToQueue(paths: string[]) {
    this.completedSession = null;
    const outcome = await invoke<{ added: number; skipped: number }>("add_paths_to_queue", { paths });
    if (outcome.added === 0) {
      toastStore.show(
        i18n.t("dragDrop.nothingToAdd", {}, "No supported audio files found to add."),
        "error"
      );
      return outcome;
    }
    if (outcome.skipped > 0) {
      toastStore.show(
        i18n.t("playerBar.tracksSkippedToast", { count: outcome.skipped }, `Skipped ${outcome.skipped} unavailable tracks.`),
        "error"
      );
    }
    await playlistsStore.refreshPlaylists();
    const qPl = playlistsStore.queuePlaylist;
    if (qPl && playlistsStore.activePlaylistId === qPl.id) {
      await playlistsStore.selectPlaylist(qPl.id);
    }
    return outcome;
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

  /** Plays every audio file under the picked folder(s) (recursively) without
   * adding the folder to the watched library — the button counterpart to
   * dropping a folder on the window. */
  async openFolderDialog() {
    try {
      const selected = await open({
        multiple: true,
        directory: true,
        title: i18n.t('emptyLibrary.playFolder'),
      });
      if (selected) {
        const paths = Array.isArray(selected) ? selected : [selected];
        if (paths.length > 0) {
          await this.openAndPlay(paths);
        }
      }
    } catch (err) {
      console.error("Failed to open folder:", err);
    }
  }

  async playSongs(songIds: number[], startIndex: number, playlistId?: number, context?: PlayContext, contextName?: string) {
    this._lastSessionSongIds = [...songIds];
    this._playedSongIdsHistory = [];
    this.completedSession = null;
    const queuePl = await playlistsStore.requireQueue();
    const effectivePlaylistId = playlistId ?? queuePl?.id;
    if (contextName) {
      this.activeContextName = contextName;
    } else if (context?.type === "album" && context.album) {
      this.activeContextName = context.album;
    } else if (context?.type === "playlist" && context.playlistId) {
      const pl = playlistsStore.playlists.find((p) => p.id === context.playlistId);
      if (pl) this.activeContextName = pl.name;
    } else if (effectivePlaylistId) {
      const pl = playlistsStore.playlists.find((p) => p.id === effectivePlaylistId);
      if (pl) this.activeContextName = pl.name;
    } else {
      this.activeContextName = undefined;
    }

    await invoke("play_songs", { songIds, startIndex, playlistId: playlistId ?? null, context: context ?? null });

    if (queuePl) {
      await playlistsStore.selectPlaylist(queuePl.id);
      await playlistsStore.refreshPlaylists();
    }
  }

  /** Replay the tracks from the just-finished queue/playlist session (#1379) */
  async replayCompletedSession() {
    if (!this.completedSession || this.completedSession.songIds.length === 0) return;
    const { songIds, contextName } = this.completedSession;
    const queuePl = await playlistsStore.requireQueue();
    await this.playSongs(songIds, 0, queuePl?.id, undefined, contextName || "Queue");
  }

  /** 1-click continuation: shuffle library tracks into the queue and begin playback (#1379) */
  async shuffleLibrary(librarySongs: Song[]) {
    if (!librarySongs || librarySongs.length === 0) return;
    const queuePl = await playlistsStore.requireQueue();
    const shuffledIds = shuffleArray(librarySongs.map((s) => s.id));
    await this.setShuffleMode("off");
    await this.playSongs(shuffledIds, 0, queuePl?.id, undefined, i18n.t("playerBar.queueTitle"));
  }

  async playPlaylistItem(playlistId: number, itemIndex: number, context?: PlayContext) {
    const pl = playlistsStore.playlists.find((p) => p.id === playlistId);
    if (pl) this.activeContextName = pl.name;
    await invoke("play_playlist_item", { playlistId, itemIndex, context: context ?? null });
  }

  async playPlaylistItemByUuid(playlistId: number, uuid: string) {
    const pl = playlistsStore.playlists.find((p) => p.id === playlistId);
    if (pl) this.activeContextName = pl.name;
    await invoke("play_playlist_item_by_uuid", { playlistId, uuid });
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
    } else if (!this.currentSong && this.completedSession) {
      await this.replayCompletedSession();
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

  /** The backend persists the flag and echoes it back via `playback-state`. */
  async setAutoContinue(enabled: boolean) {
    await invoke("set_auto_continue", { enabled });
  }

  /** Rate the current track (-1 clears; 0.5–5.0 sets rating). */
  async rateCurrent(rating: number) {
    if (!this.currentSong) return;
    this.currentSong.rating = await invoke<number>("set_song_rating", {
      songId: this.currentSong.id,
      rating,
    });
  }

  /** Set loved state for the current track (1 = loved, 0 = neutral, -1 = hated). */
  async setLovedCurrent(loved: number) {
    if (!this.currentSong) return;
    this.currentSong.loved = await invoke<number>("set_song_loved", {
      songId: this.currentSong.id,
      loved,
    });
  }

  /** Toggle loved state for the current track (0 -> 1 -> 0, or -1 -> 0). */
  async toggleLovedCurrent() {
    if (!this.currentSong) return;
    const current = this.currentSong.loved ?? 0;
    const next = current === 1 ? 0 : (current === -1 ? 0 : 1);
    await this.setLovedCurrent(next);
  }
}

export const playerStore = new PlayerStore();
