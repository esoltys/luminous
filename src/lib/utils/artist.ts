import type { AlbumItem } from "../types";
import { i18n } from "../stores/i18n.svelte";

/** An artist's albums, newest first. */
export function getArtistAlbums(albums: AlbumItem[], name: string | null): AlbumItem[] {
  if (!name) return [];
  return albums
    .filter((a) => a.artist === name)
    .sort((a, b) => (b.year ?? 0) - (a.year ?? 0));
}

/**
 * Track number for a song row: "{disc}-{track}" (e.g. "2-1") once a release
 * spans more than one disc, since a plain track number is ambiguous across
 * discs — otherwise just the track number, falling back to the row's
 * 1-based list position when the track tag itself is missing.
 */
export function formatTrackNumber(
  track: number | null | undefined,
  disc: number | null | undefined,
  fallbackIndex: number
): string {
  const trackNum = track !== undefined && track !== null ? track : fallbackIndex + 1;
  if (disc !== undefined && disc !== null && disc > 1) {
    return `${disc}-${trackNum}`;
  }
  return String(trackNum);
}

/**
 * Single card-facing category label for an album: "Single" (1 track), "EP"
 * (2-6 tracks), "Album" (7+ tracks) — overridden by "{n}-Disc Set" whenever
 * the release spans more than one disc, so a card never shows two labels.
 */
export function getAlbumCategoryLabel(trackCount: number, discCount: number | null | undefined): string {
  if ((discCount ?? 1) > 1) {
    return i18n.t("artistDetail.discSet", { count: discCount ?? 1 });
  }
  if (trackCount === 1) return i18n.t("artistDetail.single");
  if (trackCount <= 6) return i18n.t("artistDetail.ep");
  return i18n.t("artistDetail.album");
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
