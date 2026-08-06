import { invoke } from "@tauri-apps/api/core";
import type { AlbumItem, Playlist, QueuePopulationMode, Song } from "../types";
import { i18n } from "../stores/i18n.svelte";
import { isSmartPlaylistSpec } from "./filterParser";
import { collectionStore } from "../stores/collection.svelte";
import { playerStore } from "../stores/player.svelte";
import { playlistsStore } from "../stores/playlists.svelte";

export function getPopulationModeSuffix(mode: QueuePopulationMode | string | undefined | null): string {
  switch (mode) {
    case "favourites":
      return i18n.t("playlists.populationModeFavourites");
    case "familiar":
      return i18n.t("playlists.populationModeFamiliar");
    case "discover":
      return i18n.t("playlists.populationModeDiscover");
    case "deep_cuts":
      return i18n.t("playlists.populationModeDeepCuts");
    default:
      return "";
  }
}

export function getPlaylistDisplayName(
  playlist: Playlist | { name: string; population_mode?: QueuePopulationMode; dynamic_enabled?: boolean; dynamic_spec?: string } | undefined | null
): string {
  if (!playlist || !playlist.name) return "";
  const baseName = playlist.name;
  if (!playlist.dynamic_enabled || isSmartPlaylistSpec(playlist.dynamic_spec)) {
    return baseName;
  }
  const suffix = getPopulationModeSuffix(playlist.population_mode);
  return suffix ? i18n.t("playlists.populationModeTitleFormat", { base: baseName, suffix }) : baseName;
}

/**
 * Queues an album into the main Queue playlist and starts playback.
 * Shared double-click action for AlbumCard and AlbumRowCard.
 */
export async function queueAlbumAsPlaylist(album: AlbumItem): Promise<void> {
  const albumName = album.album || i18n.t("collection.unknownAlbum");
  try {
    const songs = await invoke<Song[]>("get_songs_by_album", { album: album.album || "" });
    if (songs.length > 0) {
      const songIds = songs.map((s) => s.id);
      const queuePl = await playlistsStore.ensureQueuePlaylist();
      await playerStore.playSongs(songIds, 0, queuePl?.id, undefined, albumName);
    }
  } catch (err) {
    console.error("Failed to add album to Queue:", err);
  }
}
