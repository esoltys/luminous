import { invoke } from "@tauri-apps/api/core";
import type { AlbumItem, Song } from "../types";

/** Payload of the backend's `song-stats-changed` event. */
export interface SongStatsPayload {
  song_id: number;
  rating?: number;
  loved?: number;
  playcount?: number;
  skipcount?: number;
  lastplayed?: number | null;
  not_included?: boolean;
}

/** Apply a stats event to a song object held in any view or store. */
export function applySongStats(song: Song, payload: SongStatsPayload) {
  if (typeof payload.rating === "number") song.rating = payload.rating;
  if (typeof payload.loved === "number") song.loved = payload.loved;
  if (typeof payload.playcount === "number") song.playcount = payload.playcount;
  if (typeof payload.skipcount === "number") song.skipcount = payload.skipcount;
  if (payload.lastplayed !== undefined) song.lastplayed = payload.lastplayed ?? undefined;
  if (typeof payload.not_included === "boolean") song.not_included = payload.not_included;
}

/** Payload of the backend's `album-stats-changed` event. */
export interface AlbumStatsPayload {
  album: string;
  rating?: number;
}

/** Apply an album rating event to an album item held in any view or store. */
export function applyAlbumStats(album: AlbumItem, payload: AlbumStatsPayload) {
  if (typeof payload.rating === "number") album.rating = payload.rating;
}

/**
 * Write an album's rating (-1 = unrated) through `set_album_rating` and return the value the
 * backend stored, which may differ from `rating` (it normalizes). Returns `null` when nothing
 * was written, either because there is no album name (e.g. a placeholder row) or because the
 * command failed; the failure is logged here, so callers need no try/catch and simply leave
 * their displayed rating alone on `null`.
 *
 * Callers update only their own state from the result. Other views follow the backend's
 * `album-stats-changed` event (see `applyAlbumStats`), so this does not broadcast anything.
 */
export async function rateAlbum(
  album: string | null | undefined,
  rating: number,
): Promise<number | null> {
  if (!album) return null;
  try {
    return await invoke<number>("set_album_rating", { album, rating });
  } catch (err) {
    console.error("Failed to rate album:", err);
    return null;
  }
}
