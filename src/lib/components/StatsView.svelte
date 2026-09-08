<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import LoadingSpinner from "./LoadingSpinner.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { rememberScroll } from "../utils/scrollMemory";
  import { playerStore } from "../stores/player.svelte";
  import { ChartBarIcon as BarChart2 } from "phosphor-svelte";
  import type { StatsRange, StatsSummary, StatsTopItem } from "../types";
  import { bucketListeningClock } from "../utils/listeningClock";
  import type { DaypartBucket } from "../utils/daypart";
  import { navigationStore } from "../stores/navigation.svelte";

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
  const SECTIONS: { key: keyof StatsSummary; title: () => string }[] = [
    { key: "top_artists", title: () => i18n.t("stats.topArtists", {}, "Top Artists") },
    { key: "top_albums", title: () => i18n.t("stats.topAlbums", {}, "Top Albums") },
    { key: "top_songs", title: () => i18n.t("stats.topSongs", {}, "Top Songs") },
    { key: "top_genres", title: () => i18n.t("stats.topGenres", {}, "Top Genres") }
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
  <div class="px-6 pt-8 pb-4 shrink-0">
    <h1 class="text-3xl font-heading font-bold text-brand-text-primary flex items-center gap-3">
      <BarChart2 class="w-7 h-7 text-brand-accent" />
      {i18n.t("stats.title", {}, "Stats")}
    </h1>
    <p class="text-sm text-brand-text-secondary mt-1">
      {i18n.t("stats.subtitle", {}, "Your private listening insights — computed on-device, never shared.")}
    </p>

    <div class="flex items-center gap-2 mt-4">
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
  </div>

  <div class="flex-1 overflow-y-auto px-6 pb-12" class:pb-28={!!playerStore.currentSong} use:rememberScroll={"stats"}>
    {#if loading}
      <div class="flex items-center justify-center h-64">
        <LoadingSpinner label={i18n.t("stats.loading", {}, "Loading stats...")} />
      </div>
    {:else if !summary || summary.play_timestamps.length === 0}
      <div class="flex items-center justify-center h-64 text-brand-text-secondary text-sm">
        {i18n.t("stats.empty", {}, "No listening history for this range yet.")}
      </div>
    {:else}
      <div class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-4 gap-6">
        {#each SECTIONS as section (section.key)}
          <div class="bg-brand-sidebar border border-brand-border/60 rounded-xl p-4">
            <h2 class="text-sm font-semibold text-brand-text-primary mb-3">{section.title()}</h2>
            {#if itemsFor(section.key).length === 0}
              <p class="text-xs text-brand-text-secondary">{i18n.t("stats.noData", {}, "No data for this range.")}</p>
            {:else}
              <ol class="space-y-2">
                {#each itemsFor(section.key) as item, i (item.key)}
                  {@const clickable = section.key !== "top_songs" || !!item.album}
                  <!-- svelte-ignore a11y_click_events_have_key_events -->
                  <!-- svelte-ignore a11y_no_static_element_interactions -->
                  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
                  <li
                    role={clickable ? "button" : undefined}
                    tabindex={clickable ? 0 : undefined}
                    onclick={clickable ? () => openItem(section.key, item) : undefined}
                    onkeydown={clickable
                      ? (e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); openItem(section.key, item); } }
                      : undefined}
                    class="flex items-center gap-2 text-sm rounded-md -mx-1 px-1 py-0.5 transition-colors {clickable ? 'cursor-pointer hover:bg-brand-accent/10' : ''}"
                  >
                    <span class="text-xs text-brand-text-secondary w-4 shrink-0">{i + 1}</span>
                    <div class="min-w-0 flex-1">
                      <div class="truncate text-brand-text-primary">{item.label}</div>
                      {#if item.secondary}
                        <div class="truncate text-xs text-brand-text-secondary">{item.secondary}</div>
                      {/if}
                    </div>
                    <span class="text-xs text-brand-text-secondary shrink-0">{item.play_count}</span>
                  </li>
                {/each}
              </ol>
            {/if}
          </div>
        {/each}
      </div>

      <div class="bg-brand-sidebar border border-brand-border/60 rounded-xl p-4 mt-6">
        <h2 class="text-sm font-semibold text-brand-text-primary mb-3">
          {i18n.t("stats.listeningClock", {}, "Time of Day")}
        </h2>
        <div class="grid grid-cols-4 gap-4">
          {#each CLOCK_BUCKETS as bucket (bucket.key)}
            <div class="flex flex-col items-center gap-2">
              <div class="w-full h-24 flex items-end bg-brand-main rounded-md overflow-hidden">
                <div
                  class="w-full bg-brand-accent transition-all"
                  style="height: {((clockCounts[bucket.key] ?? 0) / maxClockCount) * 100}%"
                ></div>
              </div>
              <span class="text-xs text-brand-text-secondary text-center">{bucket.label()}</span>
              <span class="text-xs text-brand-text-primary font-medium">{clockCounts[bucket.key] ?? 0}</span>
            </div>
          {/each}
        </div>
      </div>
    {/if}
  </div>
</div>
