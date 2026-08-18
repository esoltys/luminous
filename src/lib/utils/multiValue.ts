/**
 * `; `-delimited multi-value list convention shared with `songs.genre`/
 * `artist`/`album_artist`/`composer` on the backend — mirrors
 * `parse_multi_value`/`join_multi_value`/`MULTI_VALUE_DELIMITER` in
 * `src-tauri/src/models.rs`, the single source of truth for this format.
 */
export const MULTI_VALUE_DELIMITER = "; ";

/**
 * Splits a `songs.genre`/`artist`/`album_artist`/`composer`-style delimited
 * string into a clean, deduped list of individual values, trimming
 * whitespace and dropping empties.
 */
export function parseMultiValue(raw: string): string[] {
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

/** Joins a value list back into the delimited form stored in `songs.genre`/`artist`/`album_artist`/`composer`. */
export function joinMultiValue(values: string[]): string {
  return values.join(MULTI_VALUE_DELIMITER);
}
