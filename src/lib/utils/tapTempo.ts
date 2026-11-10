// Tap-tempo calculator (#1943): pure, so the Tag Editor only forwards clicks.

/** A pause longer than this starts a new measurement. */
export const TAP_RESET_MS = 2000;
/** Taps needed before a tempo is reported (two taps give one interval). */
const MIN_TAPS = 2;
/** Only the most recent taps count, so a drifting hand corrects itself. */
const MAX_TAPS = 12;
/** Tempos outside this range are almost certainly a mis-tap. */
const BPM_RANGE = [20, 400] as const;

/**
 * Records a tap at `now` (ms) after the earlier `taps`, and returns the new tap list and the tempo
 * so far. A gap of more than {@link TAP_RESET_MS} since the previous tap discards the earlier taps
 * first, so there is no separate reset to call. The tempo is the median interval's rate rounded to
 * 0.1 (a stray slow or fast tap cannot skew it), or `null` until enough taps arrive.
 */
export function registerTap(taps: readonly number[], now: number): { taps: number[]; bpm: number | null } {
  const last = taps[taps.length - 1];
  const fresh = last === undefined || now - last > TAP_RESET_MS ? [] : taps;
  const next = [...fresh, now].slice(-MAX_TAPS);
  if (next.length < MIN_TAPS) return { taps: next, bpm: null };

  const intervals = next.slice(1).map((t, i) => t - next[i]).sort((a, b) => a - b);
  const mid = Math.floor(intervals.length / 2);
  const median = intervals.length % 2 ? intervals[mid] : (intervals[mid - 1] + intervals[mid]) / 2;
  if (median <= 0) return { taps: next, bpm: null };

  const bpm = Math.round((60000 / median) * 10) / 10;
  return { taps: next, bpm: bpm >= BPM_RANGE[0] && bpm <= BPM_RANGE[1] ? bpm : null };
}
