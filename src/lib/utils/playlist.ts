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
  return suffix ? `${baseName} ${suffix}` : baseName;
}

/**
 * Queues an album into its dedicated "Album: {name}" playlist (reusing/clearing
 * it if one already exists) and starts playback. Shared double-click action
 * for AlbumCard and the compact row view — keeps both entry points in sync.
 */
export async function queueAlbumAsPlaylist(album: AlbumItem): Promise<void> {
  const albumName = album.album || i18n.t("collection.unknownAlbum");
  const playlistName = i18n.t("collection.albumPlaylistName", { name: albumName });
  const existingPlaylist = playlistsStore.playlists.find((p) => p.name === playlistName);

  if (existingPlaylist) {
    await playlistsStore.selectPlaylist(existingPlaylist.id);
    await playlistsStore.clearPlaylist(existingPlaylist.id);
  } else {
    await playlistsStore.createPlaylist(playlistName);
  }

  try {
    const songs = await invoke<Song[]>("get_songs_by_album", { album: album.album || "" });
    if (songs.length > 0 && playlistsStore.activeCustomPlaylist) {
      const songIds = songs.map((s) => s.id);
      await playlistsStore.addSongsToPlaylist(playlistsStore.activeCustomPlaylist.id, songIds);
      collectionStore.activeTab = "playlists";
      await playerStore.playPlaylistItem(playlistsStore.activeCustomPlaylist.id, 0);
    }
  } catch (err) {
    console.error("Failed to add album to playlist:", err);
  }
}
