<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import LoadingSpinner from "./LoadingSpinner.svelte";
  import ListeningHeatmap from "./ListeningHeatmap.svelte";
  import TimeOfDayGraphic from "./TimeOfDayGraphic.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { rememberScroll } from "../utils/scrollMemory";
  import { playerStore } from "../stores/player.svelte";
  import { ChartBarIcon as BarChart2 } from "phosphor-svelte";
  import type { StatsRange, StatsSummary, StatsTopItem } from "../types";
  import { bucketListeningClock } from "../utils/listeningClock";
  import type { DaypartBucket } from "../utils/daypart";
  import { navigationStore } from "../stores/navigation.svelte";
  import TopTenList from "./TopTenList.svelte";

  const VALID_RANGES: StatsRange[] = ["7d", "28d", "1y"];
  function loadSavedRange(): StatsRange {
    if (typeof window === "undefined") return "7d";
    const saved = localStorage.getItem("stats_range");
    return VALID_RANGES.includes(saved as StatsRange) ? (saved as StatsRange) : "7d";
  }

  let range = $state<StatsRange>(loadSavedRange());
  let summary = $state<StatsSummary | null>(null);
  let loading = $state(true);

  const RANGES: { value: StatsRange; label: () => string }[] = [
    { value: "7d", label: () => i18n.t("stats.range7d", {}, "Past 7 Days") },
    { value: "28d", label: () => i18n.t("stats.range28d", {}, "Past 28 Days") },
    { value: "1y", label: () => i18n.t("stats.range1y", {}, "Past Year") }
  ];

  const CLOCK_BUCKETS: { key: DaypartBucket; label: () => string }[] = [
    { key: "morning", label: () => i18n.t("stats.clockMorning", {}, "Morning") },
    { key: "afternoon", label: () => i18n.t("stats.clockAfternoon", {}, "Afternoon") },
    { key: "evening", label: () => i18n.t("stats.clockEvening", {}, "Evening") },
    { key: "latenight", label: () => i18n.t("stats.clockLateNight", {}, "Late Night") }
  ];

  let clockCounts = $derived(
    summary
      ? bucketListeningClock(summary.play_timestamps)
      : { morning: 0, afternoon: 0, evening: 0, latenight: 0 }
  );
  let maxClockCount = $derived(Math.max(1, ...Object.values(clockCounts)));

  async function loadSummary(requestedRange: StatsRange) {
    loading = true;
    try {
      const result = await invoke<StatsSummary>("get_stats_summary", { range: requestedRange });
      if (requestedRange !== range) return;
      summary = result;
    } catch (err) {
      console.error("Failed to load stats summary:", err);
      if (requestedRange === range) summary = null;
    } finally {
      if (requestedRange === range) loading = false;
    }
  }

  $effect(() => {
    loadSummary(range);
  });

  // Same order as the sidebar's Collection sub-tabs (Artists, Albums, Songs, Genres).
  const SECTIONS: { key: keyof StatsSummary; kind: "artist" | "album" | "song" | "genre"; title: () => string }[] = [
    { key: "top_artists", kind: "artist", title: () => i18n.t("stats.topArtists", {}, "Top Artists") },
    { key: "top_albums", kind: "album", title: () => i18n.t("stats.topAlbums", {}, "Top Albums") },
    { key: "top_songs", kind: "song", title: () => i18n.t("stats.topSongs", {}, "Top Songs") },
    { key: "top_genres", kind: "genre", title: () => i18n.t("stats.topGenres", {}, "Top Genres") }
  ];

  function itemsFor(key: keyof StatsSummary): StatsTopItem[] {
    if (!summary) return [];
    const value = summary[key];
    return Array.isArray(value) ? (value as StatsTopItem[]) : [];
  }

  /** Navigates to an item's detail page. Songs have no detail page of their
   * own, so a song row jumps to its album instead (`item.album`). */
  function openItem(sectionKey: keyof StatsSummary, item: StatsTopItem) {
    switch (sectionKey) {
      case "top_artists":
        navigationStore.viewArtist(item.key);
        break;
      case "top_albums":
        navigationStore.viewAlbum(item.key);
        break;
      case "top_songs":
        if (item.album) navigationStore.viewAlbum(item.album);
        break;
      case "top_genres":
        navigationStore.viewGenreTag(item.key);
        break;
    }
  }
</script>

<div class="flex-1 flex flex-col h-full bg-brand-main text-brand-text-primary select-none overflow-hidden relative">
  <div class="flex-1 overflow-y-auto px-6 pb-12" class:pb-28={!!playerStore.currentSong} use:rememberScroll={"stats"}>
    <div class="pt-8 pb-4">
      <div>
        <h1 class="text-3xl font-heading font-bold text-brand-text-primary flex items-center gap-3">
          <BarChart2 class="w-7 h-7 text-brand-accent" />
          {i18n.t("stats.title", {}, "Stats")}
        </h1>
        <p class="text-sm text-brand-text-secondary mt-1">
          {i18n.t("stats.subtitle", {}, "Your private listening insights — computed on-device, never shared.")}
        </p>
      </div>

      <div class="grid grid-cols-1 lg:grid-cols-2 gap-6 mt-6">
        <ListeningHeatmap {range} />
        <div class="bg-brand-sidebar border border-brand-border/60 rounded-xl p-4 flex flex-col">
          <h2 class="text-sm font-semibold text-brand-text-primary mb-3 shrink-0">
            {i18n.t("stats.listeningClock", {}, "Time of Day")}
          </h2>
          <div class="flex-1 min-h-0 flex flex-col justify-end">
            <TimeOfDayGraphic
              counts={clockCounts}
              max={maxClockCount}
              labels={{
                morning: CLOCK_BUCKETS[0].label(),
                afternoon: CLOCK_BUCKETS[1].label(),
                evening: CLOCK_BUCKETS[2].label(),
                latenight: CLOCK_BUCKETS[3].label()
              }}
            />
          </div>
          <div class="grid grid-cols-4 gap-4 mt-2 shrink-0">
            {#each CLOCK_BUCKETS as bucket (bucket.key)}
              <div class="flex flex-col items-center gap-0.5">
                <span class="text-xs text-brand-text-secondary text-center">{bucket.label()}</span>
                <span class="text-xs text-brand-text-primary font-medium">
                  {i18n.t("stats.minuteCount", { count: clockCounts[bucket.key] ?? 0 }, `${clockCounts[bucket.key] ?? 0} min`)}
                </span>
              </div>
            {/each}
          </div>
        </div>
      </div>

      <div class="flex items-center justify-between gap-2 mt-4">
        <div class="flex items-center gap-2">
          {#each RANGES as r (r.value)}
            <button
              onclick={() => {
                range = r.value;
                if (typeof window !== "undefined") localStorage.setItem("stats_range", r.value);
              }}
              class="px-3 py-1.5 rounded-lg text-sm font-medium transition-colors {range === r.value ? 'bg-brand-accent text-brand-accent-contrast shadow-lg shadow-brand-accent/20' : 'text-brand-text-secondary hover:bg-brand-accent/10 hover:text-brand-accent-text-hover'}"
            >
              {r.label()}
            </button>
          {/each}
        </div>
        {#if summary}
          <span class="text-sm font-medium text-brand-text-secondary shrink-0">
            {summary.total_minutes === 1
              ? i18n.t("stats.totalMinutesOne", {}, "1 minute listened")
              : i18n.t("stats.totalMinutes", { count: summary.total_minutes }, `${summary.total_minutes} minutes listened`)}
          </span>
        {/if}
      </div>
    </div>

    {#if loading}
      <div class="flex items-center justify-center h-64">
        <LoadingSpinner label={i18n.t("stats.loading", {}, "Loading stats...")} />
      </div>
    {:else if !summary || summary.play_timestamps.length === 0}
      <div class="flex items-center justify-center h-64 text-brand-text-secondary text-sm">
        {i18n.t("stats.empty", {}, "No listening history for this range yet.")}
      </div>
    {:else}
      <div class="grid grid-cols-1 lg:grid-cols-2 2xl:grid-cols-4 gap-6">
        {#each SECTIONS as section (section.key)}
          <div class="bg-brand-sidebar border border-brand-border/60 rounded-xl p-4">
            <TopTenList
              title={section.title()}
              items={itemsFor(section.key)}
              kind={section.kind}
            />
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>
