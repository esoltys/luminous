import { getCoverArtUrl } from "../types";
import { prefs } from "../stores/prefs.svelte";

/** A locally-discovered image always wins; a fetched one (cache filename,
 * served by `luminous-art://`) fills in only when there's no local file and
 * its type is still enabled in Settings › Integrations › fanart.tv (#1276) —
 * turning a type off hides what was already fetched. */
function resolveLocalOrFetched(
  localUri: string | null | undefined,
  fetchedFilename: string | null | undefined,
  fetchedEnabled: boolean
): string | null {
  return (
    getCoverArtUrl(localUri) ??
    (fetchedEnabled && fetchedFilename ? getCoverArtUrl(`luminous-art://${fetchedFilename}`) : null)
  );
}

/**
 * Resolves the display URL for an artist's portrait: a locally-discovered
 * one (artist.jpg/portrait next to the artist's music, from
 * `get_extended_artwork_for_artist`) always wins; a network-fetched one
 * (#1127, "Retrieve Artist Image") only ever fills in when no local
 * portrait exists. Shared by every place an artist portrait is rendered
 * (detail header, cards/rows, search results, share card) so they can't
 * drift on which one to prefer.
 */
export function resolveArtistPortraitUrl(
  localPortraitUri: string | null | undefined,
  fetchedImageFilename: string | null | undefined
): string | null {
  return resolveLocalOrFetched(localPortraitUri, fetchedImageFilename, prefs.fanartFetchPhoto);
}

/** The artist header's band logo: a local logo file, else a fanart.tv one (#1276). */
export function resolveArtistLogoUrl(
  localLogoUri: string | null | undefined,
  fetchedLogoFilename: string | null | undefined
): string | null {
  return resolveLocalOrFetched(localLogoUri, fetchedLogoFilename, prefs.fanartFetchLogo);
}

/** The artist header's backdrop: a local fanart file, else a fanart.tv banner (#1276). */
export function resolveArtistBackgroundUrl(
  localFanartUri: string | null | undefined,
  fetchedBackgroundFilename: string | null | undefined
): string | null {
  return resolveLocalOrFetched(localFanartUri, fetchedBackgroundFilename, prefs.fanartFetchBackground);
}

export interface CoverSource {
  id: number;
  art_manual?: string | null;
  art_automatic?: string | null;
  art_embedded?: boolean;
}

export interface CoverStackItem {
  songId?: number;
  artEmbedded?: boolean;
  artAutomatic?: string | null;
  artManual?: string | null;
}

export interface AlbumCoverSource {
  sample_song_id?: number | null;
  art_manual?: string | null;
  art_automatic?: string | null;
  art_embedded?: boolean;
}

/** Picks up to `max` visually-distinct covers (deduped by art source) from a song list, for CoverStack. */
export function songsToCoverStack(songs: CoverSource[], max = 6): CoverStackItem[] {
  const seen = new Set<string>();
  const list: CoverStackItem[] = [];
  for (const s of songs) {
    const key = s.art_manual || s.art_automatic || (s.art_embedded ? `embed-${s.id}` : null);
    if (key && !seen.has(key)) {
      seen.add(key);
      list.push({
        songId: s.id,
        artEmbedded: s.art_embedded,
        artAutomatic: s.art_automatic,
        artManual: s.art_manual,
      });
      if (list.length >= max) break;
    }
  }
  return list;
}

/**
 * An artist's covers for CoverStack, front-to-back: real per-album art first
 * (so the front tile matches the artist's actual releases), falling back to
 * covers pulled from their songs when no album has art of its own. Shared by
 * ArtistCard (the full stack) and the compact row view (just the front tile,
 * i.e. index 0) so both always agree on which cover represents the artist.
 */
export function getArtistCoverStack(
  artistAlbums: AlbumCoverSource[],
  artistSongs: CoverSource[],
  max = 6
): CoverStackItem[] {
  const albumCovers = artistAlbums
    .map((album) => ({
      songId: album.sample_song_id ?? undefined,
      artEmbedded: album.art_embedded,
      artAutomatic: album.art_automatic,
      artManual: album.art_manual,
    }))
    .filter((c) => c.artManual || c.artAutomatic || c.artEmbedded);

  if (albumCovers.length > 0) {
    return albumCovers.slice(0, max);
  }

  if (artistSongs.length > 0) {
    return songsToCoverStack(artistSongs, max);
  }

  return [];
}
