/** The four Daypart Mix buckets (#223), shared between the Home greeting
 * and the Daypart Mix auto-playlist so both always agree on "what part of
 * the day is it". Hour ranges mirror `src-tauri/src/playlist/auto_sync.rs`'s
 * `daypart_bucket_for_hour` — if one changes, the other must too. */
export type DaypartBucket = "morning" | "afternoon" | "evening" | "latenight";

/** Local-hour bucket for a given `Date` (defaults to now). */
export function getDaypartBucket(date: Date = new Date()): DaypartBucket {
  const hour = date.getHours();
  if (hour >= 5 && hour < 12) return "morning";
  if (hour >= 12 && hour < 17) return "afternoon";
  if (hour >= 17 && hour < 21) return "evening";
  return "latenight";
}
