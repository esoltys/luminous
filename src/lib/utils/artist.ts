import type { AlbumItem, Song } from "../types";
import { i18n } from "../stores/i18n.svelte";

/** An artist's albums, newest first. */
export function getArtistAlbums(albums: AlbumItem[], name: string | null): AlbumItem[] {
  if (!name) return [];
  return albums
    .filter((a) => a.artist === name)
    .sort((a, b) => (b.year ?? 0) - (a.year ?? 0));
}

/** An artist's songs, matched by either album_artist or (per-track) artist. */
export function getArtistSongs(songs: Song[], name: string | null): Song[] {
  if (!name) return [];
  const trimmed = name.trim();
  return songs.filter(
    (s) =>
      (s.album_artist && s.album_artist.trim() === trimmed) ||
      (s.artist && s.artist.trim() === trimmed)
  );
}

/**
 * Track number for a song row: "{disc}-{track}" (e.g. "1-1", "2-1") once the
 * *release* spans more than one disc — including its disc-1 tracks, so
 * numbering reads consistently across the whole release — otherwise just
 * the track number, falling back to the row's 1-based list position when
 * the track tag itself is missing. `discCount` is the release's total disc
 * count (not this song's own disc field), e.g. AlbumItem.disc_count.
 */
export function formatTrackNumber(
  track: number | null | undefined,
  disc: number | null | undefined,
  discCount: number,
  fallbackIndex: number
): string {
  const trackNum = track !== undefined && track !== null ? track : fallbackIndex + 1;
  if (discCount > 1) {
    return `${disc ?? 1}-${trackNum}`;
  }
  return String(trackNum);
}

export type ReleaseCategory = "set" | "single" | "ep" | "album";

const EP_MAX_DURATION_NANOSEC = 30 * 60 * 1_000_000_000;

/**
 * Canonical release category for an album: "set" whenever it spans more than
 * one disc (regardless of track count or duration), else "single" (1 track),
 * else "ep" (under 30 minutes) or "album" (30+ minutes) — EP/Album is a
 * duration split, not a track-count one.
 */
export function classifyRelease(
  trackCount: number,
  discCount: number | null | undefined,
  totalDurationNanosec: number | null | undefined
): ReleaseCategory {
  if ((discCount ?? 1) > 1) return "set";
  if (trackCount === 1) return "single";
  if ((totalDurationNanosec ?? 0) < EP_MAX_DURATION_NANOSEC) return "ep";
  return "album";
}

/** Single card-facing category label for an album, e.g. "{n}-Disc Set", "Single", "EP", "Album". */
export function getAlbumCategoryLabel(
  trackCount: number,
  discCount: number | null | undefined,
  totalDurationNanosec: number | null | undefined
): string {
  switch (classifyRelease(trackCount, discCount, totalDurationNanosec)) {
    case "set":
      return i18n.t("artistDetail.discSet", { count: discCount ?? 1 });
    case "single":
      return i18n.t("artistDetail.single");
    case "ep":
      return i18n.t("artistDetail.ep");
    case "album":
      return i18n.t("artistDetail.album");
  }
}

const GRADIENTS = [
  "from-indigo-600 to-purple-600",
  "from-rose-600 to-orange-600",
  "from-emerald-600 to-teal-600",
  "from-cyan-600 to-blue-600",
  "from-amber-600 to-red-600"
];

/** Deterministic fallback gradient for an artist/playlist with no cover art. */
export function getArtistGradient(name: string | null): string {
  if (!name) return "from-purple-900 to-indigo-900";
  let hash = 0;
  for (let i = 0; i < name.length; i++) {
    hash = name.charCodeAt(i) + ((hash << 5) - hash);
  }
  const index = Math.abs(hash) % GRADIENTS.length;
  return GRADIENTS[index];
}
