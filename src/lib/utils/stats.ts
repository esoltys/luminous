import type { AlbumItem, Song } from "../types";

/** Payload of the backend's `song-stats-changed` event. */
export interface SongStatsPayload {
  song_id: number;
  rating?: number;
  playcount?: number;
  skipcount?: number;
  lastplayed?: number | null;
  not_included?: boolean;
}

/** Apply a stats event to a song object held in any view or store. */
export function applySongStats(song: Song, payload: SongStatsPayload) {
  if (typeof payload.rating === "number") song.rating = payload.rating;
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
