/** The Genres page's fixed, curated 10-hue palette — colors are stored as an
 * index into this array (`TagGroup.color_index`), never a free color picker. */
export const GENRE_PALETTE_HUES = [226, 262, 298, 334, 10, 46, 82, 118, 154, 190] as const;

export function genreColorHsl(colorIndex: number): string {
  const hue = GENRE_PALETTE_HUES[((colorIndex % GENRE_PALETTE_HUES.length) + GENRE_PALETTE_HUES.length) % GENRE_PALETTE_HUES.length];
  return `hsl(${hue}, 38%, 58%)`;
}

/** A brighter, more saturated variant of the palette color, used wherever
 * text or a border needs to read clearly against a dark card background
 * (e.g. subgenre chip labels/borders) — `genreColorHsl`'s base lightness is
 * tuned for swatches and fills, not for legibility as foreground text. */
export function genreColorHslBright(colorIndex: number): string {
  const hue = GENRE_PALETTE_HUES[((colorIndex % GENRE_PALETTE_HUES.length) + GENRE_PALETTE_HUES.length) % GENRE_PALETTE_HUES.length];
  return `hsl(${hue}, 60%, 76%)`;
}
