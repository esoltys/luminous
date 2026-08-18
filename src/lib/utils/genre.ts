/**
 * `; `-delimited genre list convention shared with `songs.genre` on the
 * backend — mirrors `parse_genres`/`join_genres`/`GENRE_DELIMITER` in
 * `src-tauri/src/models.rs`, the single source of truth for this format.
 */
export const GENRE_DELIMITER = "; ";

/**
 * Splits a `songs.genre`-style delimited string into a clean, deduped list
 * of individual genres, trimming whitespace and dropping empties.
 */
export function parseGenres(raw: string): string[] {
  const seen = new Set<string>();
  const result: string[] = [];
  for (const part of raw.split(";")) {
    const trimmed = part.trim();
    if (!trimmed) continue;
    const key = trimmed.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    result.push(trimmed);
  }
  return result;
}

/** Joins a genre list back into the delimited form stored in `songs.genre`. */
export function joinGenres(genres: string[]): string {
  return genres.join(GENRE_DELIMITER);
}
