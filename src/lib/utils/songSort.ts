import type { Song } from "../types";
import { collectionStore } from "../stores/collection.svelte";

/**
 * Comparator for sorting songs by an arbitrary `Song` field, matching the
 * table's sort-header semantics: title/artist/composer/genre prefer their
 * `*sort` field when present, strings compare via localeCompare, numbers
 * compare numerically, and missing values always sort to the end regardless
 * of direction.
 */
export function compareSongs(a: Song, b: Song, field: keyof Song, asc: boolean): number {
  let valA: unknown = a[field];
  let valB: unknown = b[field];

  if (field === "title") {
    valA = a.titlesort?.trim() || a.title;
    valB = b.titlesort?.trim() || b.title;
  } else if (field === "artist") {
    valA = a.album_artist_sort?.trim() || a.artistsort?.trim() || a.album_artist || a.artist;
    valB = b.album_artist_sort?.trim() || b.artistsort?.trim() || b.album_artist || b.artist;
  } else if (field === "composer") {
    valA = a.composersort?.trim() || a.composer;
    valB = b.composersort?.trim() || b.composer;
  } else if (field === "genre") {
    valA = a.genresort?.trim() || a.genre;
    valB = b.genresort?.trim() || b.genre;
  } else if ((field as string) === "artist_tag") {
    const pA = collectionStore.getArtistProfile(a.album_artist || a.artist);
    const pB = collectionStore.getArtistProfile(b.album_artist || b.artist);
    valA = pA?.tags?.[0];
    valB = pB?.tags?.[0];
  }

  if (valA === undefined || valA === null) return asc ? 1 : -1;
  if (valB === undefined || valB === null) return asc ? -1 : 1;

  if (typeof valA === "string" && typeof valB === "string") {
    const cmp = valA.localeCompare(valB);
    return asc ? cmp : -cmp;
  }
  if (typeof valA === "number" && typeof valB === "number") {
    return asc ? valA - valB : valB - valA;
  }
  return 0;
}
