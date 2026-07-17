import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { PlaybackState, Song, ShuffleMode, RepeatMode, PlayState } from "../types";
import { themeStore } from "./theme.svelte";

export class PlayerStore {
  // Reactive state using Svelte 5 Runes
  state = $state<PlayState>("stopped");
  currentSong = $state<Song | undefined>(undefined);
  // Sticky for the session: once a track has ever loaded, the player bar
  // stays visible even if currentSong later clears (e.g. queue ends).
  hasEverPlayed = $state<boolean>(false);
  playlistId = $state<number | undefined>(undefined);
  playlistItemUuid = $state<string | undefined>(undefined);
  positionNanosec = $state<number>(0);
  volume = $state<number>(1.0);
  shuffleMode = $state<ShuffleMode>("off");
  repeatMode = $state<RepeatMode>("off");
  stopAfterCurrent = $state<boolean>(false);

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
      await listen<PlaybackState>("playback-state", (event) => {
        const oldSongId = this.currentSong?.id;
        this.updateState(event.payload);
        if (this.currentSong?.id !== oldSongId) {
          themeStore.updateArtworkColors(this.currentSong);
        }
      });

      // Listen for track changes
      await listen<{ song: Song | null }>("track-changed", (event) => {
        this.currentSong = event.payload.song || undefined;
        if (this.currentSong) this.hasEverPlayed = true;
        themeStore.updateArtworkColors(this.currentSong);
      });

      // Keep the current song's rating in sync when stats change elsewhere
      // (rating edits in list views, scrobble-point playcount bumps).
      await listen<{ song_id: number; rating?: number }>("song-stats-changed", (event) => {
        if (this.currentSong && this.currentSong.id === event.payload.song_id) {
          if (typeof event.payload.rating === "number") {
            this.currentSong.rating = event.payload.rating;
          }
        }
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

  private updateState(state: PlaybackState) {
    this.state = state.state;
    this.currentSong = state.current_song;
    if (this.currentSong) this.hasEverPlayed = true;
    this.playlistId = state.playlist_id;
    this.playlistItemUuid = state.playlist_item_uuid;
    this.positionNanosec = state.position_nanosec;
    this.volume = state.volume;
    this.shuffleMode = state.shuffle_mode;
    this.repeatMode = state.repeat_mode;
    this.stopAfterCurrent = state.stop_after_current;
  }

  // Playback Control Actions
  async playSong(songId: number) {
    await invoke("play_song", { songId });
  }

  async openAndPlay(paths: string[]) {
    await invoke("open_and_play", { paths });
  }

  async playSongs(songIds: number[], startIndex: number) {
    await invoke("play_songs", { songIds, startIndex });
  }

  async playPlaylistItem(playlistId: number, itemIndex: number) {
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
  }

  async setRepeatMode(mode: RepeatMode) {
    this.repeatMode = mode;
    await invoke("set_repeat_mode", { mode });
  }

  /** Heart = rating 5.0; unheart clears the rating. */
  async toggleFavorite() {
    if (!this.currentSong) return;
    const next = this.currentSong.rating === 5 ? -1 : 5;
    this.currentSong.rating = await invoke<number>("set_song_rating", {
      songId: this.currentSong.id,
      rating: next,
    });
  }
}

export const playerStore = new PlayerStore();
