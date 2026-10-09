import { invoke } from "@tauri-apps/api/core";
import { playerStore } from "../stores/player.svelte";
import type { Song, PlaybackState } from "../types";
import type { PlaySongSelector, ScriptPlaybackApi, ScriptWaitApi } from "./types";

/**
 * Creates the playback controller.
 * Operates over playerStore and Tauri audio IPC, guaranteeing that state transitions
 * are confirmed by real backend events.
 */
export function createPlaybackController(wait: ScriptWaitApi): ScriptPlaybackApi {
  return {
    async play(target: number | PlaySongSelector): Promise<Song> {
      let songId: number;

      if (typeof target === "number") {
        songId = target;
      } else if (target.id !== undefined) {
        songId = target.id;
      } else {
        const query = target.title ?? "";
        if (!query) {
          throw new Error("Playback play requires a song ID or a title query");
        }

        const results = await invoke<Song[]>("search_songs", { query, limit: 100 });
        const matched = results.find((s) => {
          if (target.artist && s.artist?.toLowerCase() !== target.artist.toLowerCase()) {
            return false;
          }
          if (target.title && s.title?.toLowerCase() !== target.title.toLowerCase()) {
            return false;
          }
          return true;
        }) ?? results[0];

        if (!matched) {
          throw new Error(
            `No matching song found in collection for title "${target.title}"${target.artist ? ` by "${target.artist}"` : ""}`
          );
        }
        songId = matched.id;
      }

      await invoke("play_song", { songId });

      // Await confirmation from backend that the song has started playing
      await wait.forState(
        () => playerStore.state === "playing" && playerStore.currentSong?.id === songId,
        15000
      );

      if (!playerStore.currentSong) {
        throw new Error(`Failed to confirm playback of song ID ${songId}`);
      }

      return playerStore.currentSong;
    },

    async pause(): Promise<void> {
      if (playerStore.state === "paused") return;
      await invoke("pause_playback");
      await wait.forState(() => playerStore.state === "paused", 5000);
    },

    async resume(): Promise<void> {
      if (playerStore.state === "playing") return;
      await invoke("resume_playback");
      await wait.forState(() => playerStore.state === "playing", 5000);
    },

    async seek(positionSeconds: number): Promise<void> {
      const positionNanosec = Math.round(Math.max(0, positionSeconds) * 1e9);
      await invoke("seek_to", { positionNanosec });
      await wait.settled();
    },

    status(): PlaybackState {
      return {
        state: playerStore.state,
        current_song: playerStore.currentSong,
        playlist_id: playerStore.playlistId,
        playlist_item_uuid: playerStore.playlistItemUuid,
        position_nanosec: playerStore.positionNanosec,
        volume: playerStore.volume,
        shuffle_mode: playerStore.shuffleMode,
        repeat_mode: playerStore.repeatMode,
        stop_after_current: playerStore.stopAfterCurrent,
        loudness_source: playerStore.loudnessSource,
        loudness_gain_db: playerStore.loudnessGainDb,
        remaining_playlist_items: playerStore.remainingPlaylistItems,
        auto_continue: playerStore.autoContinue,
      };
    },
  };
}
