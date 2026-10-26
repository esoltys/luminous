import { getCoverArtUrl } from "../types";
import { prefs } from "../stores/prefs.svelte";
import { albumKey } from "./albumKey";

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

/** Smaller copies `luminous-art://` serves on request (`?w=<px>`, #1528), so a
 * card decodes a cover near the size it's drawn rather than the 600 px cache
 * copy. Bounded by the backend's `MIN_SIZED_EDGE` and `CACHE_MAX_EDGE`
 * (covermanager.rs); a box bigger than the largest gets the unsized copy. */
const SIZED_COVER_EDGES = [256, 384];

/** The two forms a `luminous-art://` URL takes (WebView2 rewrites the scheme, #715). */
const ART_URL_PREFIXES = ["http://luminous-art.localhost/", "luminous-art://"];

/**
 * The URL to load a `luminous-art://` cover from.
 *
 * - `devicePixels`: the width of the box it's drawn in. A cached cover or
 *   folder-art thumbnail then loads the smallest card-sized copy that covers
 *   it (#1528); an original (`local/`, `embedded/`), a box bigger than the
 *   largest copy, or an unmeasured box (0) keeps the unsized URL.
 * - `version`: `collectionStore.coverArtVersion` (#1755). A rescan rewrites a
 *   cover under the same name, so only a changed URL makes the webview fetch
 *   it again; 0 (nothing rescanned yet) adds nothing. The backend drops it.
 *
 * Remote and mock-library URLs are returned as-is. Sizes are picked here
 * rather than by `srcset`/`sizes="auto"`, which WebKitGTK lacks and which
 * Chromium re-evaluates as `100vw` when an image is unmounted — fetching the
 * largest candidate for every card a virtualized grid drops.
 */
export function coverRequestUrl(
  url: string,
  { devicePixels = 0, version = 0 }: { devicePixels?: number; version?: number } = {}
): string {
  const prefix = ART_URL_PREFIXES.find((p) => url.startsWith(p));
  if (!prefix) return url;
  const params: string[] = [];
  const rest = url.slice(prefix.length).replace(/^localhost\//, "");
  const sizable = !rest.startsWith("local/") && !rest.startsWith("embedded/") && !rest.includes("?");
  const edge = devicePixels > 0 && sizable ? SIZED_COVER_EDGES.find((e) => e >= devicePixels) : undefined;
  if (edge) params.push(`w=${edge}`);
  if (version > 0) params.push(`v=${version}`);
  // A `?` already in `url` is part of a folder-art path, so the query always starts fresh.
  return params.length > 0 ? `${url}?${params.join("&")}` : url;
}

export interface CoverSource {
  id: number;
  album?: string | null;
  art_manual?: string | null;
  art_automatic?: string | null;
  art_embedded?: boolean;
  art_unset?: boolean;
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

/** Maps an album record (including its `sample_song_id` fallback for embedded/remote covers, #1823) to a `CoverStackItem`. */
export function albumToCoverItem(album: AlbumCoverSource): CoverStackItem {
  return {
    songId: album.sample_song_id ?? undefined,
    artEmbedded: album.art_embedded,
    artAutomatic: album.art_automatic,
    artManual: album.art_manual,
  };
}

/** Picks up to `max` visually-distinct covers (deduped by art source, falling back to distinct albums/songs by id) from a song list, for CoverStack. */
export function songsToCoverStack(songs: CoverSource[], max = 6): CoverStackItem[] {
  const seen = new Set<string>();
  const seenAlbums = new Set<string>();
  const list: CoverStackItem[] = [];
  for (const s of songs) {
    const key = s.art_manual || s.art_automatic || (s.art_embedded ? `embed-${s.id}` : null);
    if (key && !seen.has(key)) {
      seen.add(key);
      if (s.album) seenAlbums.add(albumKey(s.album));
      list.push({
        songId: s.id,
        artEmbedded: s.art_embedded,
        artAutomatic: s.art_automatic,
        artManual: s.art_manual,
      });
      if (list.length >= max) return list;
    }
  }
  for (const s of songs) {
    if (s.art_unset) continue;
    if (s.art_manual || s.art_automatic || s.art_embedded) continue;
    const normAlbum = s.album ? albumKey(s.album) : "";
    if (normAlbum && seenAlbums.has(normAlbum)) continue;
    const fallbackKey = normAlbum ? `album:${normAlbum}` : `song:${s.id}`;
    if (!seen.has(fallbackKey)) {
      seen.add(fallbackKey);
      if (normAlbum) seenAlbums.add(normAlbum);
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
 * covers pulled from their songs when no album has art of its own, and finally
 * to albums/songs carrying a representative `songId` so `<CoverArt>` can
 * resolve or fetch remote art (#1823). Shared by ArtistCard (the full stack)
 * and the compact row view (just the front tile, i.e. index 0) so both always
 * agree on which cover represents the artist.
 */
export function getArtistCoverStack(
  artistAlbums: AlbumCoverSource[],
  artistSongs: CoverSource[],
  max = 6
): CoverStackItem[] {
  const mappedAlbums = artistAlbums.map(albumToCoverItem);

  const albumCovers = mappedAlbums.filter(
    (c) => c.artManual || c.artAutomatic || c.artEmbedded
  );
  if (albumCovers.length > 0) {
    return albumCovers.slice(0, max);
  }

  const songsWithKnownArt = artistSongs.filter(
    (s) => s.art_manual || s.art_automatic || s.art_embedded
  );
  if (songsWithKnownArt.length > 0) {
    return songsToCoverStack(songsWithKnownArt, max);
  }

  const fallbackAlbumCovers = mappedAlbums.filter((c) => c.songId !== undefined);
  if (fallbackAlbumCovers.length > 0) {
    return fallbackAlbumCovers.slice(0, max);
  }

  if (artistSongs.length > 0) {
    return songsToCoverStack(artistSongs, max);
  }

  return [];
}
