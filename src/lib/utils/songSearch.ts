import type { Song } from "../types";
import { collectionStore } from "../stores/collection.svelte";

/**
 * Checks whether a song matches the active search query.
 * Supports:
 * - Free text queries (e.g. "radiohead", "creep", "1997", "rock")
 * - Multi-word queries (every word must match at least one song field or artist tag)
 * - Structured tag queries (e.g. "tag:rock", "artist-tag:canadian")
 */
export function matchesSongSearch(song: Song | undefined, rawQuery: string): boolean {
  if (!song) return false;
  const q = rawQuery.trim();
  if (!q) return true;

  // Structured artist-tag or genre-tag search
  const tagMatch = q.match(/^(?:artist[-_]?tags?|artisttags?|tags?):(.+)$/i);
  if (tagMatch) {
    const tagQuery = tagMatch[1].replace(/^['"]|['"]$/g, "").trim().toLowerCase();
    if (!tagQuery) return true;
    const artistName = (song.album_artist || song.artist)?.toLowerCase();
    if (artistName) {
      const profile = collectionStore.artistProfiles[artistName];
      if (profile?.tags?.some((t) => t.toLowerCase().includes(tagQuery))) return true;
    }
    if (song.genre?.toLowerCase().includes(tagQuery)) return true;
    return false;
  }

  const terms = q.toLowerCase().split(/\s+/).filter(Boolean);
  if (terms.length === 0) return true;

  const artistName = (song.album_artist || song.artist)?.toLowerCase();
  const profileTags = artistName ? collectionStore.artistProfiles[artistName]?.tags : undefined;

  return terms.every((term) => {
    if (song.title?.toLowerCase().includes(term)) return true;
    if (song.artist?.toLowerCase().includes(term)) return true;
    if (song.album_artist?.toLowerCase().includes(term)) return true;
    if (song.album?.toLowerCase().includes(term)) return true;
    if (song.genre?.toLowerCase().includes(term)) return true;
    if (song.composer?.toLowerCase().includes(term)) return true;
    if (song.year && String(song.year).includes(term)) return true;
    if (song.path?.toLowerCase().includes(term)) return true;
    if (profileTags?.some((t) => t.toLowerCase().includes(term))) return true;
    return false;
  });
}
