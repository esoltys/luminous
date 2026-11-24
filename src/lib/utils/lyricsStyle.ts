/**
 * Dimmed states for synced lyrics, shared by the miniplayer and the full lyrics view.
 *
 * Fading the lines and words that are not current is the point of karaoke-style lyrics, so these
 * are the one deliberate use of alpha on theme text. Everywhere else, readable text uses the
 * theme text colours at full strength (docs/UI_CONVENTIONS.md "Readable text has no alpha");
 * `scripts/lint-text-alpha.ts` skips this file for that reason. Keep the class names written out
 * in full so Tailwind can see them.
 */

/** A line that is not the one currently being sung. */
export const LYRIC_LINE_IDLE = "text-brand-text-secondary/35 hover:text-brand-text-secondary/60";

/** A word of the current line that has not been sung yet. */
export const LYRIC_WORD_UNSUNG = "text-brand-text-primary/40 opacity-40";
