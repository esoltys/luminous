import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { PlaybackState, Song, ShuffleMode, RepeatMode, PlayState } from "../types";
import { themeStore } from "./theme.svelte";

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
        themeStore.updateArtworkColors(this.currentSong);
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
}

export const playerStore = new PlayerStore();
