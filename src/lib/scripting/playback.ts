import { invoke } from "@tauri-apps/api/core";
import { playerStore } from "../stores/player.svelte";
import {
  playingStationFor,
  stationUuidOf,
  type PlayingStation,
  type RadioDirectoryStation,
  type ScriptPlaybackSnapshot,
  type Song,
} from "../types";
import type { PlaySongSelector, ScriptPlaybackApi, ScriptWaitApi } from "./types";

/** Finds a station by name in the downloaded directory: exact name first, then the top search hit. */
async function findStationByName(name: string): Promise<RadioDirectoryStation> {
  const results = await invoke<RadioDirectoryStation[]>("query_radio_directory", {
    query: { search: name, hideUnsupported: false, limit: 50 },
  });
  const wanted = name.toLowerCase();
  const match = results.find((s) => s.name.trim().toLowerCase() === wanted) ?? results[0];
  if (!match) {
    throw new Error(
      `No station matching "${name}" in the station directory (is it downloaded on the Radio tab?)`,
    );
  }
  return match;
}

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
        const matched =
          results.find((s) => {
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
            `No matching song found in collection for title "${target.title}"${target.artist ? ` by "${target.artist}"` : ""}`,
          );
        }
        songId = matched.id;
      }

      await invoke("play_song", { songId });

      // Await confirmation from backend that the song has started playing
      await wait.forState(
        () => playerStore.state === "playing" && playerStore.currentSong?.id === songId,
        15000,
      );

      if (!playerStore.currentSong) {
        throw new Error(`Failed to confirm playback of song ID ${songId}`);
      }

      return playerStore.currentSong;
    },

    async playStation(target: string | { name: string }): Promise<PlayingStation> {
      let uuid: string;
      let station: RadioDirectoryStation | null;
      if (typeof target === "string") {
        uuid = target.trim();
        if (!uuid) throw new Error("playStation requires a station uuid or { name }");
        // A saved or pinned station pruned from the directory still plays from its
        // stored row, so a miss here is not an error; play_radio_station decides.
        station = await invoke<RadioDirectoryStation | null>("get_radio_directory_station", {
          stationuuid: uuid,
        });
      } else {
        const name = target.name?.trim();
        if (!name) throw new Error("playStation requires a station uuid or { name }");
        station = await findStationByName(name);
        uuid = station.stationuuid;
      }

      if (station && !station.playable) {
        throw new Error(
          `Station "${station.name}" uses a stream format the audio engine can't play (HLS or HE-AAC)`,
        );
      }

      // Called directly rather than through radioStore.playStation, which turns
      // failures into a toast: a script needs the rejection.
      await invoke("play_radio_station", { stationuuid: uuid });
      await wait.forState(
        () => playerStore.state === "playing" && stationUuidOf(playerStore.currentSong) === uuid,
        20000,
      );

      const playing = playingStationFor(playerStore.currentSong);
      if (!playing) throw new Error(`Failed to confirm playback of station ${uuid}`);
      return playing;
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

    status(): ScriptPlaybackSnapshot {
      return {
        state: playerStore.state,
        current_song: playerStore.currentSong,
        current_station: playingStationFor(playerStore.currentSong),
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
