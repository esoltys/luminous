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
