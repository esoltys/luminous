import type { AlbumItem } from "../types";

/** An artist's albums, newest first. */
export function getArtistAlbums(albums: AlbumItem[], name: string | null): AlbumItem[] {
  if (!name) return [];
  return albums
    .filter((a) => a.artist === name)
    .sort((a, b) => (b.year ?? 0) - (a.year ?? 0));
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
