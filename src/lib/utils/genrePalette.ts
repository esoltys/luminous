/** The Genres page's fixed, curated 10-hue palette — colors are stored as an
 * index into this array (`TagGroup.color_index`), never a free color picker. */
export const GENRE_PALETTE_HUES = [226, 262, 298, 334, 10, 46, 82, 118, 154, 190] as const;

export function genreColorHsl(colorIndex: number): string {
  const hue = GENRE_PALETTE_HUES[((colorIndex % GENRE_PALETTE_HUES.length) + GENRE_PALETTE_HUES.length) % GENRE_PALETTE_HUES.length];
  return `hsl(${hue}, 38%, 58%)`;
}
