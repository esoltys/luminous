import type { TagGroup } from "../types";

/** The Genres page's fixed, curated 10-hue palette — colors are stored as an
 * index into this array (`TagGroup.color_index`), never a free color picker. */
export const GENRE_PALETTE_HUES = [226, 262, 298, 334, 10, 46, 82, 118, 154, 190] as const;

/**
 * Resolves the curated color index for a genre/tag name, given the current
 * hierarchy: a direct `tag_groups` match wins first; otherwise falls back to
 * whichever card currently curates it as a child (a chip's own color always
 * follows its parent card's — see #545/#548). Returns `undefined` if `name`
 * isn't found anywhere in the hierarchy (e.g. a genre below the auto-playlist
 * threshold, or a stale reference), letting the caller fall back to a
 * default color rather than crashing.
 */
export function resolveGenreColorIndex(hierarchy: TagGroup[], name: string): number | undefined {
  const direct = hierarchy.find((g) => g.name === name);
  if (direct) return direct.color_index;
  const parent = hierarchy.find((g) => g.children.some((c) => c.name === name));
  return parent?.color_index;
}

export function genreColorHsl(colorIndex: number): string {
  const hue = GENRE_PALETTE_HUES[((colorIndex % GENRE_PALETTE_HUES.length) + GENRE_PALETTE_HUES.length) % GENRE_PALETTE_HUES.length];
  return `hsl(${hue}, 38%, 58%)`;
}

/** A brighter, more saturated variant of the palette color, used wherever
 * text or a border needs to read clearly against a dark card background
 * (e.g. subgenre chip labels/borders) — `genreColorHsl`'s base lightness is
 * tuned for swatches and fills, not for legibility as foreground text.
 * Only legible on a dark theme's near-black chip fill — see
 * `genreColorHslDark` for the light-theme counterpart. */
export function genreColorHslBright(colorIndex: number): string {
  const hue = GENRE_PALETTE_HUES[((colorIndex % GENRE_PALETTE_HUES.length) + GENRE_PALETTE_HUES.length) % GENRE_PALETTE_HUES.length];
  return `hsl(${hue}, 60%, 76%)`;
}

/** A darker, saturated variant of the palette color — the light-theme
 * counterpart to `genreColorHslBright`. On a light theme the chip fill is a
 * pale tint of the card's light background, so a light/bright foreground
 * color would be nearly invisible; this stays dark enough to read clearly
 * as text/border against that pale fill. */
export function genreColorHslDark(colorIndex: number): string {
  const hue = GENRE_PALETTE_HUES[((colorIndex % GENRE_PALETTE_HUES.length) + GENRE_PALETTE_HUES.length) % GENRE_PALETTE_HUES.length];
  return `hsl(${hue}, 55%, 30%)`;
}
