<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { playerStore } from "../stores/player.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import type { HomeItem, StatsTopItem, TopAlbumItem, ScanProgress } from "../types";
  import HomeRowList from "./HomeRowList.svelte";
  import TopTenList from "./TopTenList.svelte";
  import PinnedRow from "./PinnedRow.svelte";
  import LibraryWelcome from "./LibraryWelcome.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { rememberScroll } from "../utils/scrollMemory";
  import { getDaypartBucket } from "../utils/daypart";
  import { formatWeekRange } from "../utils/date";

  let topAlbums = $state<StatsTopItem[]>([]);
  let topAlbumsPeriodStart = $state<number | null>(null);
  let recentlyAdded = $state<HomeItem[]>([]);
  let featuredAlbums = $state<HomeItem[]>([]);
  let isLoading = $state(true);
  let libraryChangedDebounce: ReturnType<typeof setTimeout> | undefined;

  /** Polled rather than computed once, so the greeting (and the Daypart Mix
   * pin's implicit "current bucket") actually flips while the user sits on
   * Home across a boundary, instead of only updating on the next unrelated
   * re-render (#223). Matches playlists.svelte.ts's own boundary-check
   * cadence. */
  let daypartBucket = $state(getDaypartBucket());
  let daypartPollTimer: ReturnType<typeof setInterval> | undefined;

  const topAlbumsTitle = $derived.by((): string => {
    const base = i18n.t('home.topAlbums');
    return topAlbumsPeriodStart === null
      ? base
      : `${base} ${formatWeekRange(topAlbumsPeriodStart, i18n.currentLocale)}`;
  });

  const timeOfDayGreeting = $derived.by((): string => {
    switch (daypartBucket) {
      case "morning": return i18n.t("home.greetingMorning");
      case "afternoon": return i18n.t("home.greetingAfternoon");
      case "evening": return i18n.t("home.greetingEvening");
      case "latenight": return i18n.t("home.greetingNight");
    }
  });

  /** Maps the weekly chart's `TopAlbumItem` (rank + movement, #662) onto the
   * generic `StatsTopItem` shape `TopTenList` renders, carrying the movement
   * fields through as extras so the rank column can show a trend icon. */
  function toStatsTopItem(item: TopAlbumItem): StatsTopItem {
    const { album } = item;
    return {
      key: album.album ?? "",
      label: album.album ?? "",
      secondary: album.artist,
      play_count: 0,
      minutes: 0,
      excluded: false,
      album: null,
      sample_song_id: album.sample_song_id,
      art_embedded: album.art_embedded,
      art_automatic: album.art_automatic,
      art_manual: album.art_manual,
      year: album.year,
      rating: album.rating,
      movement: item.movement as StatsTopItem["movement"],
      previous_rank: item.previous_rank,
      peak_rank: item.peak_rank,
      weeks_on_chart: item.weeks_on_chart,
    };
  }

  async function loadCuratedData() {
    isLoading = true;
    try {
      const [top, added, featured] = await Promise.all([
        invoke<TopAlbumItem[]>("get_top_albums", { limit: 10 }),
        invoke<HomeItem[]>("get_recently_added", { limit: 10 }),
        invoke<HomeItem[]>("get_featured_albums", { limit: 5 }),
      ]);
      topAlbums = top.map(toStatsTopItem);
      topAlbumsPeriodStart = top[0]?.period_start ?? null;
      recentlyAdded = added;
      featuredAlbums = featured;
    } catch (err) {
      console.error("Failed to load curated data:", err);
    } finally {
      isLoading = false;
    }
  }

  onMount(() => {
    loadCuratedData();

    const unlistenScan = listen<ScanProgress>("scan-progress", (event) => {
      if (event.payload.phase === "done") loadCuratedData();
    });

    const unlistenLibrary = listen("library-changed", () => {
      clearTimeout(libraryChangedDebounce);
      libraryChangedDebounce = setTimeout(loadCuratedData, 500);
    });

    daypartPollTimer = setInterval(() => {
      daypartBucket = getDaypartBucket();
    }, 60_000);

    return () => {
      clearTimeout(libraryChangedDebounce);
      clearInterval(daypartPollTimer);
      unlistenScan.then((fn) => fn());
      unlistenLibrary.then((fn) => fn());
    };
  });
</script>

<div class="flex flex-col h-full w-full bg-brand-main overflow-hidden">
  <div class="flex-1 overflow-y-auto {playerStore.currentSong ? 'pb-28' : 'pb-6'}" use:rememberScroll={"home"}>
    <div class="px-6 pt-6">
      <h1 class="text-3xl sm:text-4xl font-heading font-bold text-brand-text-primary leading-snug py-0.5">
        {timeOfDayGreeting}
      </h1>
    </div>

    <div class="px-6 pt-4 space-y-12">
    {#if isLoading}
      <div class="flex items-center justify-center h-64">
        <div class="text-brand-text-secondary">{i18n.t('home.loading')}</div>
      </div>
    {:else}
      <PinnedRow />

      {#if topAlbums.length > 0 || featuredAlbums.length > 0 || recentlyAdded.length > 0}
        <div class="grid grid-cols-1 lg:grid-cols-2 gap-8">
          {#if topAlbums.length > 0}
            <TopTenList
              title={topAlbumsTitle}
              items={topAlbums}
              kind="album"
              secondaryFallback={i18n.t('collection.variousArtists')}
              showDuration={false}
              onHeaderClick={() => { navigationStore.activeTab = "stats"; }}
            />
          {:else if featuredAlbums.length > 0}
            <HomeRowList title={i18n.t('home.exploreLibrary')} items={featuredAlbums} variant="added" />
          {/if}
          {#if recentlyAdded.length > 0}
            <HomeRowList
              title={i18n.t('home.recentlyAdded')}
              items={recentlyAdded}
              variant="added"
              onHeaderClick={() => navigationStore.viewAutoPlaylist({ kind: "recently_added" })}
            />
          {/if}
        </div>
      {/if}

      {#if topAlbums.length === 0 && recentlyAdded.length === 0}
        <div class="flex items-center justify-center py-16">
          <LibraryWelcome />
        </div>
      {/if}
    {/if}
    </div>
  </div>
</div>

<style>
  :global(.home-view-scroll) {
    scrollbar-width: thin;
    scrollbar-color: var(--color-border) transparent;
  }
  :global(.home-view-scroll::-webkit-scrollbar) {
    width: 6px;
  }
  :global(.home-view-scroll::-webkit-scrollbar-track) {
    background: transparent;
  }
  :global(.home-view-scroll::-webkit-scrollbar-thumb) {
    background: var(--color-border);
    border-radius: 3px;
  }
</style>
