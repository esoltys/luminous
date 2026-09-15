<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { FlameIcon, QuestionIcon } from "phosphor-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { prefs } from "../stores/prefs.svelte";
  import type { ListenEvent } from "../types";
  import { bucketDailyMinutes, buildHeatmapGrid, computeStreaks, localDateKey, type HeatmapCell } from "../utils/listeningHeatmap";

  // 14 weeks (~3 months) — "a couple months wide" per #890 — plus enough
  // history for a meaningful longest-streak calculation without pulling a
  // user's entire listening history on every Stats view load.
  const WEEKS = 14;
  const LOOKBACK_DAYS = WEEKS * 7;

  // Cell background intensity per HeatmapCell.level, expressed as opacity
  // steps of the active theme's accent color (`--color-brand-accent`) rather
  // than a fixed palette, so the heatmap reskins correctly across every
  // color theme and in both light/dark mode (#890). Reused for the legend
  // swatches so "Less → More" matches the grid exactly.
  const LEVEL_OPACITY = [0, 25, 45, 70, 100];

  let events = $state<ListenEvent[] | null>(null);
  let hovered = $state<HeatmapCell | null>(null);

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

  // Grid always includes today's cell (`buildHeatmapGrid` pads out to the
  // end of the current week), so the status line can default to it without
  // a separate "today" fetch.
  let todayCell = $derived(rows.flat().find((cell) => cell.date === localDateKey(new Date())) ?? null);
  let displayCell = $derived(hovered ?? todayCell);

  // Sparse day-of-week labels (every other row) to avoid crowding the
  // 11px cells; which literal weekdays land on odd rows depends on the
  // "Start the week with" preference, so this isn't always Mon/Wed/Fri.
  let dayLabels = $derived(
    rows.map((row, rowIndex) => {
      if (rowIndex % 2 === 0) return "";
      const cell = row[row.length - 1];
      if (!cell) return "";
      return dateFromKey(cell.date).toLocaleDateString(i18n.currentLocale, { weekday: "narrow" });
    })
  );

  // A column gets a month label when it contains that month's 1st.
  let monthLabels = $derived(
    columns.map((column) => {
      const firstOfMonth = column.find((cell) => !cell.future && cell.date.endsWith("-01"));
      if (!firstOfMonth) return "";
      return dateFromKey(firstOfMonth.date).toLocaleDateString(i18n.currentLocale, { month: "short" });
    })
  );

  function dateFromKey(key: string): Date {
    const [y, m, d] = key.split("-").map(Number);
    return new Date(y, m - 1, d);
  }

  function cellStyle(cell: HeatmapCell): string {
    if (cell.future) return "background-color: transparent;";
    const opacity = LEVEL_OPACITY[cell.level];
    if (opacity === 0) return "background-color: color-mix(in srgb, var(--color-brand-accent) 12%, transparent);";
    return `background-color: color-mix(in srgb, var(--color-brand-accent) ${opacity}%, transparent);`;
  }

  function cellLabel(cell: HeatmapCell): string {
    const date = dateFromKey(cell.date).toLocaleDateString(i18n.currentLocale, { weekday: "short", month: "short", day: "numeric" });
    if (cell.minutes <= 0) return i18n.t("stats.heatmapStatusNoListening", { date }, `${date} — no listening`);
    if (cell.minutes === 1) return i18n.t("stats.heatmapStatusOneMinute", { date }, `${date} — 1 minute listened`);
    return i18n.t("stats.heatmapStatusMinutes", { minutes: cell.minutes, date }, `${date} — ${cell.minutes} minutes listened`);
  }

  function streakLabel(days: number): string {
    return days === 1 ? i18n.t("stats.heatmapStreakOneDay") : i18n.t("stats.heatmapStreakDays", { count: days });
  }
</script>

<div class="bg-brand-sidebar border border-brand-border/60 rounded-xl p-4 shrink-0">
  <div class="flex items-center justify-between gap-2">
    <h2 class="text-sm font-semibold text-brand-text-primary">{i18n.t("stats.heatmapTitle", {}, "Listening Streak")}</h2>
    <span title={i18n.t("stats.heatmapHelp", {}, "This calendar shows how many days in a row you've listened to music. Each square is one day — darker squares mean more minutes listened that day. Hover or tap a square to see its date.")} class="inline-flex cursor-help">
      <QuestionIcon class="w-4 h-4 text-brand-text-secondary/60 hover:text-brand-text-secondary" />
    </span>
  </div>

  {#if events}
    <div class="flex items-center gap-4 mt-3">
      <FlameIcon weight="fill" class="w-8 h-8 text-brand-accent shrink-0" />
      <div class="flex items-center gap-4">
        <div>
          <div class="text-2xl font-bold text-brand-text-primary leading-tight">{streakLabel(streaks.current)}</div>
          <div class="text-xs text-brand-text-secondary">{i18n.t("stats.heatmapCurrentStreak", {}, "Current streak")}</div>
        </div>
        <div class="w-px h-8 bg-brand-border"></div>
        <div>
          <div class="text-2xl font-bold text-brand-text-primary leading-tight">{streakLabel(streaks.longest)}</div>
          <div class="text-xs text-brand-text-secondary">{i18n.t("stats.heatmapLongestStreak", {}, "Longest streak")}</div>
        </div>
      </div>
    </div>

    <div class="flex gap-3 mt-4">
      <div class="flex flex-col gap-[3px] pt-4 text-[10px] text-brand-text-secondary/70 leading-none">
        {#each dayLabels as label, i (i)}
          <div class="w-3 h-[11px] flex items-center">{label}</div>
        {/each}
      </div>
      <div role="group" aria-label={i18n.t("stats.heatmapTitle", {}, "Listening Streak")} onmouseleave={() => (hovered = null)}>
        <div class="flex gap-[3px] text-[10px] text-brand-text-secondary/70 leading-none mb-1">
          {#each monthLabels as label, colIndex (colIndex)}
            <div class="w-[11px]">{label}</div>
          {/each}
        </div>
        <div class="flex gap-[3px]">
          {#each columns as column, colIndex (colIndex)}
            <div class="flex flex-col gap-[3px]">
              {#each column as cell (cell.date)}
                <button
                  type="button"
                  class="w-[11px] h-[11px] rounded-sm"
                  style={cellStyle(cell)}
                  disabled={cell.future}
                  aria-label={cell.future ? undefined : cellLabel(cell)}
                  onmouseenter={() => !cell.future && (hovered = cell)}
                  onfocus={() => !cell.future && (hovered = cell)}
                  onclick={() => !cell.future && (hovered = cell)}
                ></button>
              {/each}
            </div>
          {/each}
        </div>
      </div>
    </div>

    <div class="flex items-center justify-between gap-4 mt-3 pt-3 border-t border-brand-border/60">
      <span class="text-xs text-brand-text-secondary truncate">{displayCell ? cellLabel(displayCell) : ""}</span>
      <div class="flex items-center gap-1.5 text-[10px] text-brand-text-secondary/70 shrink-0">
        <span>{i18n.t("stats.heatmapLegendLess", {}, "Less")}</span>
        {#each LEVEL_OPACITY as opacity (opacity)}
          <div
            class="w-[11px] h-[11px] rounded-sm"
            style={opacity === 0
              ? "background-color: color-mix(in srgb, var(--color-brand-accent) 12%, transparent);"
              : `background-color: color-mix(in srgb, var(--color-brand-accent) ${opacity}%, transparent);`}
          ></div>
        {/each}
        <span>{i18n.t("stats.heatmapLegendMore", {}, "More")}</span>
      </div>
    </div>
  {/if}
</div>
