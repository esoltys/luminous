import { getDaypartBucket, type DaypartBucket } from "./daypart";

/** Buckets raw unix-second play timestamps into local-time Morning/
 * Afternoon/Evening/Late Night counts, reusing the same `DaypartBucket`
 * hour boundaries as the Home greeting and the "Late Night Mix" auto-
 * playlist (#223) so the Personal Stats (#130) listening clock always
 * agrees with "what part of the day is it" elsewhere in the app.
 *
 * Bucketing happens entirely client-side (`Date.getHours()` uses the
 * browser's timezone) rather than via a server-side UTC-offset param
 * passed to the backend — simpler, no DST logic, and immune to
 * stale-offset bugs if the user's timezone changes between listen-time
 * and view-time. */
export function bucketListeningClock(timestamps: number[]): Record<DaypartBucket, number> {
  const counts: Record<DaypartBucket, number> = { morning: 0, afternoon: 0, evening: 0, latenight: 0 };
  for (const ts of timestamps) {
    counts[getDaypartBucket(new Date(ts * 1000))]++;
  }
  return counts;
}
