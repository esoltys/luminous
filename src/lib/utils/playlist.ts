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

// BPM auto-playlists are our own fixed category labels (like Favourites/Recently
// Added/History), not user library data (unlike genre/decade), so their display
// name must come from i18n rather than the English name the backend stores as
// dynamic_spec's playlists.name — this maps each bucket's dynamic_spec suffix
// (e.g. "60-90") to its i18n key. Mirrors src-tauri/src/playlist.rs's BPM_BUCKETS.
const BPM_RANGE_TO_I18N_KEY: Record<string, string> = {
  "60-90": "bpmDownTempo",
  "90-115": "bpmMidTempo",
  "115-130": "bpmUptempo",
  "130-150": "bpmHighEnergy",
  "150-": "bpmExtreme",
};

/**
 * Localizes a BPM auto-playlist's bucket name from its `dynamic_spec`
 * (e.g. `"bpmrange:60-90"`), falling back to `fallbackName` (the raw,
 * English playlist row name) for specs that don't map to a known bucket.
 */
export function getBpmBucketLabel(dynamicSpec: string | undefined | null, fallbackName: string): string {
  const bpmRange = dynamicSpec?.startsWith("bpmrange:") ? dynamicSpec.slice("bpmrange:".length) : undefined;
  const bpmKey = bpmRange !== undefined ? BPM_RANGE_TO_I18N_KEY[bpmRange] : undefined;
  return bpmKey ? i18n.t(`playlists.${bpmKey}`) : fallbackName;
}

export function getPlaylistDisplayName(
  playlist: Playlist | { name: string; population_mode?: QueuePopulationMode; dynamic_enabled?: boolean; dynamic_spec?: string } | undefined | null
): string {
  if (!playlist || !playlist.name) return "";
  // Genre auto-playlists (#548) are keyed one row per curated tag, so
  // `name` is already the plain display name — no per-spec label
  // derivation needed here the way the old bare-genre-string convention
  // required (a chip's own card already exists separately, so there's
  // nothing to strip a parent's name out of).
  const baseName = getBpmBucketLabel(playlist.dynamic_spec, playlist.name);
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
      const queuePl = await playlistsStore.requireQueue();
      await playerStore.playSongs(songIds, 0, queuePl.id, undefined, albumName);
    }
  } catch (err) {
    console.error("Failed to add album to Queue:", err);
  }
}
