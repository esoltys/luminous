import type { StatsTopItem } from "../types";

/**
 * Each item's length as a 0-100 share of the list's leader, for the
 * proportional accent bars behind Top N rows (#1475). Ranks by minutes, or by
 * play count when the leader has none. Shared by the Stats lists and the share
 * cards so both draw the same bar for the same row.
 */
export function statsBarPercents(items: Pick<StatsTopItem, "minutes" | "play_count">[]): number[] {
  if (!items || items.length === 0) return [];
  const top = items[0];
  const anyMinutes = Math.max(0, ...items.map((it) => it.minutes || 0)) > 0;
  const useMinutes = top.minutes > 0 ? true : top.play_count > 0 ? false : anyMinutes;
  const maxMetric =
    top.minutes > 0
      ? top.minutes
      : top.play_count > 0
        ? top.play_count
        : anyMinutes
          ? Math.max(0, ...items.map((it) => it.minutes || 0))
          : Math.max(0, ...items.map((it) => it.play_count || 0));
  if (maxMetric <= 0) return items.map(() => 0);
  return items.map((it) => Math.min(100, Math.max(0, ((useMinutes ? it.minutes : it.play_count) / maxMetric) * 100)));
}
