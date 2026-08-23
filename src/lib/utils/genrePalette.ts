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
