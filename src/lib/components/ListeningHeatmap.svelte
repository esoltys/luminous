<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { FlameIcon, StarIcon } from "phosphor-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { prefs } from "../stores/prefs.svelte";
  import type { ListenEvent, StatsRange } from "../types";
  import {
    bucketDailyMinutes,
    buildHeatmapGrid,
    buildLastNDays,
    computeStreaks,
    localDateKey,
    type HeatmapCell
  } from "../utils/listeningHeatmap";

  interface Props {
    range: StatsRange;
  }

  let { range }: Props = $props();

  // Grid cells are a fixed size; for the 1-year view the number of weeks
  // shown grows/shrinks with the measured width (ResizeObserver) instead of
  // stretching cell size to fill it (which made them oversized/blocky on
  // wide windows). The 28-day view has a fixed week count by definition, so
  // it stretches cell *width* instead (see `fillCellsToWidth`).
  const CELL_PX = 13;
  const GAP_PX = 3;
  const MIN_WEEKS = 8;
  const MAX_WEEKS = 52;
  const BAR_DAYS = 7;
  const GRID_WEEKS_28D = 4;

  // Cell/bar intensity per HeatmapCell.level, expressed as opacity steps of
  // the active theme's accent color (`--color-brand-accent`) rather than a
  // fixed palette, so it reskins correctly across every color theme and in
  // both light/dark mode (#890). Reused for the legend swatches so
  // "Less → More" matches the grid/bars exactly.
  const LEVEL_OPACITY = [0, 25, 45, 70, 100];

  let events = $state<ListenEvent[] | null>(null);
  let hovered = $state<HeatmapCell | null>(null);
  let gridEl = $state<HTMLDivElement | undefined>(undefined);
  let gridWidth = $state(0);

  // "Past 7 Days" reads as a bar chart (each day is visually distinct
  // rather than a single sparse grid row); "Past 28 Days" reads as a
  // traditional month-style calendar (7 days across, one row per week);
  // "Past Year" stays the GitHub-style heatmap (weeks across, 7 days
  // tall) — matching the Stats page's other range-aware panels.
  let mode = $derived(range === "7d" ? "bar" : range === "28d" ? "calendar" : "heatmap");
  let fixedWeeks = $derived(range === "28d" ? GRID_WEEKS_28D : MAX_WEEKS);
  let lookbackDays = $derived(mode === "bar" ? BAR_DAYS : fixedWeeks * 7);

  async function load(days: number) {
    try {
      events = await invoke<ListenEvent[]>("get_listening_activity", { days });
    } catch (err) {
      console.error("Failed to load listening activity:", err);
      events = [];
    }
  }

  $effect(() => {
    load(lookbackDays);
  });

  $effect(() => {
    if (!gridEl) return;
    const observer = new ResizeObserver(([entry]) => {
      if (entry) gridWidth = entry.contentRect.width;
    });
    observer.observe(gridEl);
    return () => observer.disconnect();
  });

  // Only the heatmap (1-year) view extends how much history is *shown* to
  // fill extra width (more weeks); the calendar (28-day) view has a fixed
  // week count and instead fills width via CSS grid's equal columns.
  let visibleWeeks = $derived(
    mode === "heatmap"
      ? gridWidth > 0
        ? Math.min(MAX_WEEKS, Math.max(MIN_WEEKS, Math.floor((gridWidth + GAP_PX) / (CELL_PX + GAP_PX))))
        : MIN_WEEKS
      : fixedWeeks
  );

  // The backend's `days` cutoff is an exact `now - days*86400s` timestamp,
  // not a calendar-day boundary, so it can spill a few hours into one extra
  // calendar date beyond what's actually displayed. Trim back to exactly
  // `lookbackDays` calendar days so streaks/peak-day don't count a stray
  // boundary day that isn't shown anywhere in the card.
  let dailyMinutes = $derived.by(() => {
    if (!events) return new Map<string, number>();
    const full = bucketDailyMinutes(events);
    const cutoff = new Date();
    cutoff.setDate(cutoff.getDate() - (lookbackDays - 1));
    const cutoffKey = localDateKey(cutoff);
    return new Map([...full].filter(([date]) => date >= cutoffKey));
  });
  let streaks = $derived(computeStreaks(dailyMinutes));

  let barDays = $derived(mode === "bar" ? buildLastNDays(dailyMinutes, BAR_DAYS) : []);
  let barMax = $derived(Math.max(1, ...barDays.map((d) => d.minutes)));

  // Bars should grow up from zero whenever the bar chart itself newly
  // appears (switching into the 7d range, or landing on Stats already there)
  // rather than snapping straight to their final heights. `barsReady` tracks
  // "the bar chart is actually on screen" (data loaded + range is 7d);
  // flipping `barsRevealed` false then true a frame later re-triggers the
  // CSS height transition below each time that happens.
  let barsReady = $derived(!!events && mode === "bar");
  let barsRevealed = $state(false);

  $effect(() => {
    if (!barsReady) {
      barsRevealed = false;
      return;
    }
    barsRevealed = false;
    const raf = requestAnimationFrame(() => { barsRevealed = true; });
    return () => cancelAnimationFrame(raf);
  });

  // `buildHeatmapGrid` returns rows (day-of-week) x columns (week). The
  // heatmap view renders `columns` as vertical stacks; the calendar view
  // renders each entry in `columns` as a horizontal week row instead — same
  // data, different orientation. Always built at the full fetched range so
  // streak math sees it all; only the rendered `columns` below are trimmed
  // to what currently fits (heatmap) or fixed (calendar).
  let rows = $derived(mode !== "bar" ? buildHeatmapGrid(dailyMinutes, fixedWeeks, prefs.weekStart) : []);
  let allColumns = $derived(
    mode !== "bar" ? Array.from({ length: fixedWeeks }, (_, col) => rows.map((row) => row[col])) : []
  );
  let columns = $derived(allColumns.slice(allColumns.length - visibleWeeks));

  // Grid always includes today's cell (`buildHeatmapGrid` pads out to the
  // end of the current week; `buildLastNDays` always ends today), so the
  // status line can default to it without a separate "today" fetch.
  let todayKey = localDateKey(new Date());
  let todayCell = $derived(
    mode === "bar" ? (barDays.find((cell) => cell.date === todayKey) ?? null) : (rows.flat().find((cell) => cell.date === todayKey) ?? null)
  );
  let displayCell = $derived(hovered ?? todayCell);

  // Full day-of-week abbreviations (Sun/Mon/…) for the heatmap's trailing
  // sidebar, one per row — picked by actual calendar weekday (`getDay()`)
  // off the most recent column, so the labels follow the current "Start
  // the week with" preference regardless of which row they land on.
  let dayLabels = $derived(
    mode === "heatmap"
      ? rows.map((row) => {
          const cell = row[row.length - 1];
          if (!cell) return "";
          return dateFromKey(cell.date).toLocaleDateString(i18n.currentLocale, { weekday: "short" });
        })
      : []
  );

  // A column gets a month label when it contains that month's 1st.
  let monthLabels = $derived(
    mode === "heatmap"
      ? columns.map((column) => {
          const firstOfMonth = column.find((cell) => !cell.future && cell.date.endsWith("-01"));
          if (!firstOfMonth) return "";
          return dateFromKey(firstOfMonth.date).toLocaleDateString(i18n.currentLocale, { month: "short" });
        })
      : []
  );

  // Calendar view's header row: full weekday names (not sparse, plenty of
  // room at 7-wide), in the same "Start the week with" order as its rows —
  // read off the most recent week so the labels are always in sync with it.
  let calendarDayLabels = $derived(
    mode === "calendar" && columns.length > 0
      ? columns[columns.length - 1].map((cell) => dateFromKey(cell.date).toLocaleDateString(i18n.currentLocale, { weekday: "short" }))
      : []
  );

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

  // Zero-minute days still render a thin sliver so every bar stays visible
  // and clickable rather than collapsing to nothing.
  function barHeightPercent(cell: HeatmapCell): number {
    if (cell.minutes <= 0) return 4;
    return Math.max(8, Math.round((cell.minutes / barMax) * 100));
  }
</script>

<div class="bg-brand-sidebar border border-brand-border/60 rounded-xl p-4">
  <h2 class="text-sm font-semibold text-brand-text-primary">{i18n.t("stats.heatmapTitle", {}, "Listening Streak")}</h2>

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

    {#if mode === "bar"}
      <div
        role="group"
        aria-label={i18n.t("stats.heatmapTitle", {}, "Listening Streak")}
        class="flex items-stretch gap-2 mt-8 h-20"
        onmouseleave={() => (hovered = null)}
      >
        {#each barDays as cell (cell.date)}
          <button
            type="button"
            class="flex-1 flex flex-col items-center gap-1"
            aria-label={cellLabel(cell)}
            onmouseenter={() => (hovered = cell)}
            onfocus={() => (hovered = cell)}
            onclick={() => (hovered = cell)}
          >
            <div class="flex-1 w-full flex items-end">
              <div
                class="relative w-full rounded-t-sm transition-[height] duration-500 ease-out"
                style="height: {barsRevealed ? barHeightPercent(cell) : 0}%; {cellStyle(cell)}"
              >
                {#if cell.date === peakDate}
                  <StarIcon
                    weight="fill"
                    class="absolute top-1 left-1/2 -translate-x-1/2 w-[10px] h-[10px] text-brand-text-primary drop-shadow-[0_0_1px_rgba(0,0,0,0.8)]"
                  />
                {/if}
                {#if cell.minutes > 0}
                  <span
                    class="absolute -top-4 left-1/2 -translate-x-1/2 text-[10px] leading-none whitespace-nowrap text-brand-text-primary/70"
                  >
                    {i18n.t("stats.minuteCount", { count: cell.minutes }, `${cell.minutes} min`)}
                  </span>
                {/if}
              </div>
            </div>
            <span class="text-[10px] text-brand-text-secondary/70">
              {dateFromKey(cell.date).toLocaleDateString(i18n.currentLocale, { weekday: "short" })}
            </span>
          </button>
        {/each}
      </div>
    {:else if mode === "calendar"}
      <div class="mt-4">
        <div class="grid grid-cols-7 gap-[3px] text-[10px] text-brand-text-secondary/70 leading-none mb-1">
          {#each calendarDayLabels as label, i (i)}
            <div class="text-center">{label}</div>
          {/each}
        </div>
        <div
          role="group"
          aria-label={i18n.t("stats.heatmapTitle", {}, "Listening Streak")}
          class="flex flex-col gap-[3px]"
          onmouseleave={() => (hovered = null)}
        >
          {#each columns as week, weekIndex (weekIndex)}
            <div class="grid grid-cols-7 gap-[3px]">
              {#each week as cell (cell.date)}
                <button
                  type="button"
                  class="relative flex flex-col items-center justify-center gap-0.5 aspect-square rounded-sm"
                  style={cellStyle(cell)}
                  disabled={cell.future}
                  aria-label={cell.future ? undefined : cellLabel(cell)}
                  onmouseenter={() => !cell.future && (hovered = cell)}
                  onfocus={() => !cell.future && (hovered = cell)}
                  onclick={() => !cell.future && (hovered = cell)}
                >
                  {#if cell.date === peakDate}
                    <StarIcon weight="fill" class="w-[28%] h-[28%] text-brand-text-primary drop-shadow-[0_0_1px_rgba(0,0,0,0.8)]" />
                  {:else if !cell.future}
                    <span class="text-xs font-medium leading-none text-brand-text-primary/70">{dateFromKey(cell.date).getDate()}</span>
                  {/if}
                  {#if !cell.future && cell.minutes > 0}
                    <span class="text-[9px] leading-none text-brand-text-primary/60">
                      {i18n.t("stats.minuteCount", { count: cell.minutes }, `${cell.minutes} min`)}
                    </span>
                  {/if}
                </button>
              {/each}
            </div>
          {/each}
        </div>
      </div>
    {:else}
      <div class="flex gap-1 mt-4">
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
                    class="relative flex items-center justify-center w-[13px] h-[13px] rounded-sm"
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
        <div class="flex flex-col text-[10px] text-brand-text-secondary/70 leading-none shrink-0">
          <div class="invisible mb-1" aria-hidden="true">&nbsp;</div>
          <div class="flex flex-col gap-[3px]">
            {#each dayLabels as label, i (i)}
              <div class="h-[13px] flex items-center whitespace-nowrap">{label}</div>
            {/each}
          </div>
        </div>
      </div>
    {/if}

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
