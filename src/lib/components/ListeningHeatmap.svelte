<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { i18n } from "../stores/i18n.svelte";
  import { prefs } from "../stores/prefs.svelte";
  import type { ListenEvent } from "../types";
  import { bucketDailyMinutes, buildHeatmapGrid, computeStreaks, type HeatmapCell } from "../utils/listeningHeatmap";

  // 14 weeks (~3 months) — "a couple months wide" per #890 — plus enough
  // history for a meaningful longest-streak calculation without pulling a
  // user's entire listening history on every Stats view load.
  const WEEKS = 14;
  const LOOKBACK_DAYS = WEEKS * 7;

  // Cell background intensity per HeatmapCell.level, expressed as opacity
  // steps of the active theme's accent color (`--color-brand-accent`) rather
  // than a fixed palette, so the heatmap reskins correctly across every
  // color theme and in both light/dark mode (#890).
  const LEVEL_OPACITY = [0, 25, 45, 70, 100];

  let events = $state<ListenEvent[] | null>(null);

  async function load() {
    try {
      events = await invoke<ListenEvent[]>("get_listening_activity", { days: LOOKBACK_DAYS });
    } catch (err) {
      console.error("Failed to load listening activity:", err);
      events = [];
    }
  }

  $effect(() => {
    load();
  });

  let dailyMinutes = $derived(events ? bucketDailyMinutes(events) : new Map<string, number>());
  // `buildHeatmapGrid` returns rows (day-of-week) x columns (week); rendered
  // here as columns of 7 stacked cells, so transpose before drawing.
  let rows = $derived(buildHeatmapGrid(dailyMinutes, WEEKS, prefs.weekStart));
  let columns = $derived(Array.from({ length: WEEKS }, (_, col) => rows.map((row) => row[col])));
  let streaks = $derived(computeStreaks(dailyMinutes));

  function cellStyle(cell: HeatmapCell): string {
    if (cell.future) return "background-color: transparent;";
    const opacity = LEVEL_OPACITY[cell.level];
    if (opacity === 0) return "background-color: color-mix(in srgb, var(--color-brand-accent) 12%, transparent);";
    return `background-color: color-mix(in srgb, var(--color-brand-accent) ${opacity}%, transparent);`;
  }

  function cellTitle(cell: HeatmapCell): string {
    if (cell.future) return "";
    const [y, m, d] = cell.date.split("-").map(Number);
    const date = new Date(y, m - 1, d).toLocaleDateString(i18n.currentLocale, { month: "short", day: "numeric" });
    if (cell.minutes <= 0) return i18n.t("stats.heatmapTooltipNoListening", { date });
    if (cell.minutes === 1) return i18n.t("stats.heatmapTooltipOneMinute", { date });
    return i18n.t("stats.heatmapTooltipMinutes", { minutes: cell.minutes, date });
  }

  function streakLabel(days: number): string {
    return days === 1 ? i18n.t("stats.heatmapStreakOneDay") : i18n.t("stats.heatmapStreakDays", { count: days });
  }
</script>

<div class="flex flex-col items-end gap-2">
  {#if events}
    <div class="flex gap-[3px]">
      {#each columns as column, colIndex (colIndex)}
        <div class="flex flex-col gap-[3px]">
          {#each column as cell (cell.date)}
            <div class="w-[11px] h-[11px] rounded-sm" style={cellStyle(cell)} title={cellTitle(cell)}></div>
          {/each}
        </div>
      {/each}
    </div>
    <div class="flex items-center gap-4 text-xs text-brand-text-secondary">
      <span>{i18n.t("stats.heatmapCurrentStreak", {}, "Current Streak")}: <span class="text-brand-text-primary font-medium">{streakLabel(streaks.current)}</span></span>
      <span>{i18n.t("stats.heatmapLongestStreak", {}, "Longest Streak")}: <span class="text-brand-text-primary font-medium">{streakLabel(streaks.longest)}</span></span>
    </div>
  {/if}
</div>
