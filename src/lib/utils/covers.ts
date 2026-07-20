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
