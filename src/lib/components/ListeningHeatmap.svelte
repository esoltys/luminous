<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { FlameIcon, QuestionIcon, StarIcon } from "phosphor-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { prefs } from "../stores/prefs.svelte";
  import type { ListenEvent } from "../types";
  import { bucketDailyMinutes, buildHeatmapGrid, computeStreaks, localDateKey, type HeatmapCell } from "../utils/listeningHeatmap";

  // Cells are a fixed size; the number of weeks shown grows or shrinks with
  // the grid's measured width instead of stretching cells to fill it (which
  // made them oversized/blocky on wide windows). MAX_WEEKS bounds both how
  // much history is fetched and how far the grid can extend on very wide
  // windows; MIN_WEEKS is the fallback before the first width measurement.
  const CELL_PX = 13;
  const GAP_PX = 3;
  const MIN_WEEKS = 8;
  const MAX_WEEKS = 52;
  const LOOKBACK_DAYS = MAX_WEEKS * 7;

  // Cell background intensity per HeatmapCell.level, expressed as opacity
  // steps of the active theme's accent color (`--color-brand-accent`) rather
  // than a fixed palette, so the heatmap reskins correctly across every
  // color theme and in both light/dark mode (#890). Reused for the legend
  // swatches so "Less → More" matches the grid exactly.
  const LEVEL_OPACITY = [0, 25, 45, 70, 100];

  let events = $state<ListenEvent[] | null>(null);
  let hovered = $state<HeatmapCell | null>(null);
  let gridEl = $state<HTMLDivElement | undefined>(undefined);
  let gridWidth = $state(0);

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

  $effect(() => {
    if (!gridEl) return;
    const observer = new ResizeObserver(([entry]) => {
      if (entry) gridWidth = entry.contentRect.width;
    });
    observer.observe(gridEl);
    return () => observer.disconnect();
  });

  // How many weeks fit in the measured grid width at CELL_PX, clamped to
  // [MIN_WEEKS, MAX_WEEKS].
  let visibleWeeks = $derived(
    gridWidth > 0
      ? Math.min(MAX_WEEKS, Math.max(MIN_WEEKS, Math.floor((gridWidth + GAP_PX) / (CELL_PX + GAP_PX))))
      : MIN_WEEKS
  );

  let dailyMinutes = $derived(events ? bucketDailyMinutes(events) : new Map<string, number>());
  // `buildHeatmapGrid` returns rows (day-of-week) x columns (week); rendered
  // here as columns of 7 stacked cells, so transpose before drawing. Always
  // built at MAX_WEEKS so streak math sees the full fetched history; only
  // the rendered `columns` below are trimmed to what currently fits.
  let rows = $derived(buildHeatmapGrid(dailyMinutes, MAX_WEEKS, prefs.weekStart));
  let allColumns = $derived(Array.from({ length: MAX_WEEKS }, (_, col) => rows.map((row) => row[col])));
  let columns = $derived(allColumns.slice(allColumns.length - visibleWeeks));
  let streaks = $derived(computeStreaks(dailyMinutes));

  // The single highest-minutes day gets a star marker (a "special day"
  // callout, not a leaderboard/trophy — see AGENTS.md's icon-semantics
  // convention). No star when there's no listening at all yet.
  let peakDate = $derived.by(() => {
    let best: string | null = null;
    let bestMinutes = 0;
    for (const [date, minutes] of dailyMinutes) {
      if (minutes > bestMinutes) {
        best = date;
        bestMinutes = minutes;
      }
    }
    return best;
  });

  // Grid always includes today's cell (`buildHeatmapGrid` pads out to the
  // end of the current week), so the status line can default to it without
  // a separate "today" fetch.
  let todayCell = $derived(rows.flat().find((cell) => cell.date === localDateKey(new Date())) ?? null);
  let displayCell = $derived(hovered ?? todayCell);

  // Sparse day-of-week labels, always Mon/Wed/Fri regardless of which row
  // that lands on for the current "Start the week with" preference — picked
  // by actual calendar weekday (`getDay()`), not row parity, so the letters
  // stay put whether the grid starts on Sunday or Monday.
  const LABELED_WEEKDAYS = new Set([1, 3, 5]); // Mon, Wed, Fri
  let dayLabels = $derived(
    rows.map((row) => {
      const cell = row[row.length - 1];
      if (!cell) return "";
      const date = dateFromKey(cell.date);
      if (!LABELED_WEEKDAYS.has(date.getDay())) return "";
      return date.toLocaleDateString(i18n.currentLocale, { weekday: "narrow" });
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
    return i18n.t("stats.heatmapStatus", { date, minutes: cell.minutes }, `${date} — ${cell.minutes} min`);
  }

  function streakLabel(days: number): string {
    return days === 1 ? i18n.t("stats.heatmapStreakOneDay") : i18n.t("stats.heatmapStreakDays", { count: days });
  }
</script>

<div class="bg-brand-sidebar border border-brand-border/60 rounded-xl p-4">
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
      <div class="flex flex-col text-[10px] text-brand-text-secondary/70 leading-none shrink-0">
        <div class="invisible mb-1" aria-hidden="true">&nbsp;</div>
        <div class="flex flex-col gap-[3px]">
          {#each dayLabels as label, i (i)}
            <div class="w-3 h-[13px] flex items-center">{label}</div>
          {/each}
        </div>
      </div>
      <div
        bind:this={gridEl}
        role="group"
        aria-label={i18n.t("stats.heatmapTitle", {}, "Listening Streak")}
        class="flex-1 min-w-0 overflow-hidden"
        onmouseleave={() => (hovered = null)}
      >
        <div class="flex gap-[3px] text-[10px] text-brand-text-secondary/70 leading-none mb-1">
          {#each monthLabels as label, colIndex (colIndex)}
            <div class="w-[13px] shrink-0">{label}</div>
          {/each}
        </div>
        <div class="flex gap-[3px]">
          {#each columns as column, colIndex (colIndex)}
            <div class="flex flex-col gap-[3px] shrink-0">
              {#each column as cell (cell.date)}
                <button
                  type="button"
                  class="flex items-center justify-center w-[13px] h-[13px] rounded-sm"
                  style={cellStyle(cell)}
                  disabled={cell.future}
                  aria-label={cell.future ? undefined : cellLabel(cell)}
                  onmouseenter={() => !cell.future && (hovered = cell)}
                  onfocus={() => !cell.future && (hovered = cell)}
                  onclick={() => !cell.future && (hovered = cell)}
                >
                  {#if cell.date === peakDate}
                    <StarIcon weight="fill" class="w-[9px] h-[9px] text-brand-text-primary drop-shadow-[0_0_1px_rgba(0,0,0,0.8)]" />
                  {/if}
                </button>
              {/each}
            </div>
          {/each}
        </div>
      </div>
    </div>

    <div class="flex items-center justify-between gap-4 mt-3 pt-3 border-t border-brand-border/60">
      <span class="text-xs text-brand-text-secondary truncate min-w-0">{displayCell ? cellLabel(displayCell) : ""}</span>
      <div class="flex items-center gap-1.5 text-[10px] text-brand-text-secondary/70 shrink-0">
        <span>{i18n.t("stats.heatmapLegendLess", {}, "Less")}</span>
        {#each LEVEL_OPACITY as opacity (opacity)}
          <div
            class="w-[13px] h-[13px] rounded-sm"
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
