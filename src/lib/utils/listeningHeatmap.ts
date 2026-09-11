import type { ListenEvent } from "../types";
import type { WeekStart } from "../stores/prefs.svelte";

const MS_PER_DAY = 24 * 60 * 60 * 1000;

export interface HeatmapCell {
  /** Local calendar date, YYYY-MM-DD. */
  date: string;
  /** Total minutes of music played on this day. */
  minutes: number;
  /** Intensity bucket for cell coloring: 0 = no listening. */
  level: 0 | 1 | 2 | 3 | 4;
  /** True for a cell past today, included only to keep every week's column
   * full-height — always renders empty regardless of `minutes`. */
  future: boolean;
}

export interface StreakInfo {
  current: number;
  longest: number;
}

function localDateKey(date: Date): string {
  const y = date.getFullYear();
  const m = String(date.getMonth() + 1).padStart(2, "0");
  const d = String(date.getDate()).padStart(2, "0");
  return `${y}-${m}-${d}`;
}

function startOfDay(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate());
}

function intensityLevel(minutes: number): HeatmapCell["level"] {
  if (minutes <= 0) return 0;
  if (minutes < 30) return 1;
  if (minutes < 60) return 2;
  if (minutes < 120) return 3;
  return 4;
}

/** Sums each listen event's duration into its local calendar day, in whole
 * minutes. Bucketing happens entirely client-side (`Date` uses the browser's
 * timezone), mirroring `bucketListeningClock`'s rationale — no server-side
 * UTC-offset day math, immune to DST/timezone changes between listen-time
 * and view-time. */
export function bucketDailyMinutes(events: ListenEvent[]): Map<string, number> {
  const seconds = new Map<string, number>();
  for (const event of events) {
    const key = localDateKey(new Date(event.played_at * 1000));
    seconds.set(key, (seconds.get(key) ?? 0) + event.duration_secs);
  }
  const minutes = new Map<string, number>();
  for (const [key, secs] of seconds) {
    minutes.set(key, Math.floor(secs / 60));
  }
  return minutes;
}

/** Builds a `weeks`-wide grid of days ending today, reshaped into 7 rows
 * (one per day-of-week) x `weeks` columns. Row order follows the
 * "Start the week with" preference (`weekStart`, Sunday- or Monday-first —
 * see `prefs.svelte.ts`) rather than a hardcoded Sun-Sat order, matching how
 * `formatChartWeekRange` already respects this setting elsewhere. */
export function buildHeatmapGrid(
  dailyMinutes: Map<string, number>,
  weeks: number,
  weekStart: WeekStart,
  today: Date = new Date()
): HeatmapCell[][] {
  const todayStart = startOfDay(today);
  const todayDow = weekStart === "sunday" ? todayStart.getDay() : (todayStart.getDay() + 6) % 7;
  // Pad the grid out to full weeks: the last column ends on the
  // weekStart-aligned last day of the current week (today plus however many
  // days remain in it), so every column is a complete 7-day week.
  const daysAfterToday = 6 - todayDow;
  const gridEnd = new Date(todayStart);
  gridEnd.setDate(gridEnd.getDate() + daysAfterToday);

  const totalDays = weeks * 7;
  const cells: HeatmapCell[] = [];
  for (let i = totalDays - 1; i >= 0; i--) {
    const date = new Date(gridEnd);
    date.setDate(date.getDate() - i);
    const key = localDateKey(date);
    const future = date.getTime() > todayStart.getTime();
    const minutes = future ? 0 : (dailyMinutes.get(key) ?? 0);
    cells.push({ date: key, minutes, level: intensityLevel(minutes), future });
  }

  // Reshape into rows (day-of-week) x columns (week): `cells` is chronological
  // starting on a weekStart-aligned day, so index i is (dayOfWeek = i % 7,
  // week = floor(i / 7)).
  const rows: HeatmapCell[][] = Array.from({ length: 7 }, () => []);
  cells.forEach((cell, i) => rows[i % 7].push(cell));
  return rows;
}

function countBackwardFrom(activeDays: Set<string>, start: Date): number {
  let count = 0;
  const cursor = new Date(start);
  while (activeDays.has(localDateKey(cursor))) {
    count++;
    cursor.setDate(cursor.getDate() - 1);
  }
  return count;
}

/** Current and longest listening streaks (consecutive days with any minutes
 * played), in whole days. The current streak allows one day of grace: if
 * today has no listening yet, it counts backward from yesterday instead of
 * resetting to zero mid-day, matching how most streak trackers behave. */
export function computeStreaks(dailyMinutes: Map<string, number>, today: Date = new Date()): StreakInfo {
  const activeDays = new Set(
    [...dailyMinutes.entries()].filter(([, minutes]) => minutes > 0).map(([key]) => key)
  );
  if (activeDays.size === 0) return { current: 0, longest: 0 };

  const todayStart = startOfDay(today);
  let current = countBackwardFrom(activeDays, todayStart);
  if (current === 0) {
    const yesterday = new Date(todayStart);
    yesterday.setDate(yesterday.getDate() - 1);
    current = countBackwardFrom(activeDays, yesterday);
  }

  const sortedDays = [...activeDays].sort();
  let longest = 0;
  let run = 0;
  let prevDate: Date | null = null;
  for (const key of sortedDays) {
    const [y, m, d] = key.split("-").map(Number);
    const date = new Date(y, m - 1, d);
    run = prevDate && date.getTime() - prevDate.getTime() === MS_PER_DAY ? run + 1 : 1;
    longest = Math.max(longest, run);
    prevDate = date;
  }

  return { current, longest: Math.max(longest, current) };
}
