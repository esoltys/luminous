import { invoke } from "@tauri-apps/api/core";
import type { AlbumItem, AutoPlaylistItem, Playlist, PlaylistItem, QueuePopulationMode, Song } from "../types";
import { i18n } from "../stores/i18n.svelte";
import { isSmartPlaylistSpec } from "./filterParser";
import { collectionStore } from "../stores/collection.svelte";
import { playerStore } from "../stores/player.svelte";
import { playlistsStore } from "../stores/playlists.svelte";
import { toastStore } from "../stores/toast.svelte";

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
  // Missing Metadata (#367) is a diagnostic singleton, not a library-data
  // category — a population-mode suffix ("Missing Metadata (Favourites)")
  // wouldn't mean anything useful, so it's excluded the same way Smart
  // Playlists are.
  if (
    !playlist.dynamic_enabled ||
    isSmartPlaylistSpec(playlist.dynamic_spec) ||
    playlist.dynamic_spec === "missingmeta"
  ) {
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

export async function queueArtistAsPlaylist(artistName: string): Promise<void> {
  const name = artistName || i18n.t("collection.unknownArtist");
  try {
    const songs = await invoke<Song[]>("get_songs_by_artist", { artist: artistName || "" });
    const playable = songs.filter((s) => !s.not_included && !s.unavailable);
    if (playable.length > 0) {
      const queuePl = await playlistsStore.requireQueue();
      await playerStore.setShuffleMode("off");
      await playerStore.playSongs(playable.map((s) => s.id), 0, queuePl?.id, undefined, name);
    }
  } catch (err) {
    console.error("Failed to play artist:", err);
  }
}

export async function addArtistToQueue(artistName: string): Promise<void> {
  try {
    const songs = await invoke<Song[]>("get_songs_by_artist", { artist: artistName || "" });
    const playable = songs.filter((s) => !s.not_included && !s.unavailable);
    if (playable.length > 0) {
      await playlistsStore.addSongsToQueue(playable.map((s) => s.id));
      const name = artistName || i18n.t("collection.unknownArtist");
      toastStore.show(i18n.t("playlists.addedToQueueSuccess", { name }, `Added ${name} to Queue`));
    }
  } catch (err) {
    console.error("Failed to add artist to Queue:", err);
  }
}

export async function queuePlaylistAsPlaylist(playlist: Playlist): Promise<void> {
  const name = getPlaylistDisplayName(playlist);
  try {
    const items = await invoke<PlaylistItem[]>("get_playlist_tracks", { playlistId: playlist.id });
    const playable = items.filter((t) => t.song && !t.song.not_included && !t.song.unavailable).map((t) => t.song as Song);
    if (playable.length > 0) {
      const queuePl = await playlistsStore.requireQueue();
      await playerStore.setShuffleMode("off");
      await playerStore.playSongs(playable.map((s) => s.id), 0, queuePl?.id, undefined, name);
    }
  } catch (err) {
    console.error("Failed to play playlist:", err);
  }
}

export async function addPlaylistToQueue(playlist: Playlist): Promise<void> {
  try {
    const items = await invoke<PlaylistItem[]>("get_playlist_tracks", { playlistId: playlist.id });
    const playable = items.filter((t) => t.song && !t.song.not_included && !t.song.unavailable).map((t) => t.song as Song);
    if (playable.length > 0) {
      await playlistsStore.addSongsToQueue(playable.map((s) => s.id));
      const name = getPlaylistDisplayName(playlist);
      toastStore.show(i18n.t("playlists.addedToQueueSuccess", { name }, `Added ${name} to Queue`));
    }
  } catch (err) {
    console.error("Failed to add playlist to Queue:", err);
  }
}

async function fetchAutoPlaylistSongs(ap: AutoPlaylistItem): Promise<Song[]> {
  const { kind, genre, decade, bpm, artistTag, playlistId } = ap;
  if (
    (kind === "genre" || kind === "decade" || kind === "bpm" || kind === "artist_tag" || kind === "missing_metadata" || kind === "daypart") &&
    playlistId !== undefined
  ) {
    const items = await invoke<PlaylistItem[]>("get_playlist_tracks", { playlistId });
    return items.filter((item) => !!item.song && !item.song.not_included && !item.song.unavailable).map((item) => item.song as Song);
  }
  let songs: Song[] = [];
  if (kind === "favourites") songs = await invoke<Song[]>("get_favourite_songs");
  else if (kind === "recently_added") songs = await invoke<Song[]>("get_recently_added_songs", { limit: 50 });
  else if (kind === "most_played") songs = await invoke<Song[]>("get_most_played_songs", { limit: 50 });
  else if (kind === "history") songs = await invoke<Song[]>("get_recently_played_songs", { limit: 100 });
  else if (kind === "decade") songs = await invoke<Song[]>("get_songs_by_decade", { decade: decade ?? "", limit: 50 });
  else if (kind === "bpm") songs = await invoke<Song[]>("get_songs_by_bpm", { spec: bpm ?? "", limit: 50 });
  else if (kind === "artist_tag") songs = await invoke<Song[]>("get_songs_by_artist_tag", { tag: artistTag ?? "", limit: 500 });
  else songs = await invoke<Song[]>("get_songs_by_curated_tag", { tagName: genre ?? "", limit: 500 });
  return songs.filter((s) => !s.not_included && !s.unavailable);
}

export async function queueAutoPlaylistAsPlaylist(ap: AutoPlaylistItem, label: string): Promise<void> {
  try {
    const playable = await fetchAutoPlaylistSongs(ap);
    if (playable.length > 0) {
      const queuePl = await playlistsStore.requireQueue();
      await playerStore.setShuffleMode("off");
      await playerStore.playSongs(playable.map((s) => s.id), 0, queuePl?.id, undefined, label);
    }
  } catch (err) {
    console.error("Failed to play auto-playlist:", err);
  }
}

export async function addAutoPlaylistToQueue(ap: AutoPlaylistItem, label: string): Promise<void> {
  try {
    const playable = await fetchAutoPlaylistSongs(ap);
    if (playable.length > 0) {
      await playlistsStore.addSongsToQueue(playable.map((s) => s.id));
      toastStore.show(i18n.t("playlists.addedToQueueSuccess", { name: label }, `Added ${label} to Queue`));
    }
  } catch (err) {
    console.error("Failed to add auto-playlist to Queue:", err);
  }
}

