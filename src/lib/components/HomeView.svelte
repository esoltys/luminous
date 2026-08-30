<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { playerStore } from "../stores/player.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import type { HomeItem, ArtistItem, Song, ScanProgress } from "../types";
  import { getArtistAlbums, getArtistSongs } from "../utils/artist";
  import HorizontalScrollRow from "./HorizontalScrollRow.svelte";
  import ArtistCard from "./ArtistCard.svelte";
  import HomeRowList from "./HomeRowList.svelte";
  import LibraryWelcome from "./LibraryWelcome.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { rememberScroll } from "../utils/scrollMemory";

  let topArtists = $state<ArtistItem[]>([]);
  let frequentlyPlayed = $state<HomeItem[]>([]);
  let recentlyAdded = $state<HomeItem[]>([]);
  let featuredAlbums = $state<HomeItem[]>([]);
  let isLoading = $state(true);
  let libraryChangedDebounce: ReturnType<typeof setTimeout> | undefined;

  /** Routes "Top Artists" to Collection → Artists, pre-sorted by popularity
   * (total plays). CollectionView reads its initial sort from localStorage
   * on mount, so writing it here before navigating is enough — see #169. */
  function viewTopArtists() {
    if (typeof window !== "undefined") {
      localStorage.setItem("sort_artist_field", "total_playcount");
      localStorage.setItem("sort_artist_asc", "false");
    }
    collectionStore.searchQuery = "";
    collectionStore.searchResults = [];
    navigationStore.selectedArtistName = null;
    navigationStore.selectedAlbumName = null;
    navigationStore.activeTab = "collection";
    navigationStore.activeSubTab = "artists";
  }

  function getTimeOfDayGreeting(): string {
    const hour = new Date().getHours();
    if (hour >= 5 && hour < 12) return i18n.t("home.greetingMorning");
    if (hour >= 12 && hour < 17) return i18n.t("home.greetingAfternoon");
    if (hour >= 17 && hour < 21) return i18n.t("home.greetingEvening");
    return i18n.t("home.greetingNight");
  }

  async function loadCuratedData() {
    isLoading = true;
    try {
      const [artists, frequent, added, featured] = await Promise.all([
        invoke<ArtistItem[]>("get_top_artists", { limit: 15 }),
        invoke<Song[]>("get_most_played_songs", { limit: 5 }),
        invoke<HomeItem[]>("get_recently_added", { limit: 5 }),
        invoke<HomeItem[]>("get_featured_albums", { limit: 5 }),
      ]);
      topArtists = artists;
      // Flat per-song ranking (same definition/query as the "Most Played"
      // auto-playlist, see #169) rather than the old context-grouped mix of
      // Album/Song/Playlist cards — that grouping could show a whole album
      // as "most played" from a single repeatedly-played track.
      frequentlyPlayed = frequent.map((song) => ({ type: "song", song }) as const);
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

    return () => {
      clearTimeout(libraryChangedDebounce);
      unlistenScan.then((fn) => fn());
      unlistenLibrary.then((fn) => fn());
    };
  });
</script>

<div class="flex flex-col h-full w-full bg-brand-main overflow-hidden">
  <div class="flex-1 overflow-y-auto {playerStore.currentSong ? 'pb-28' : 'pb-6'}" use:rememberScroll={"home"}>
    <div class="px-6 pt-8">
      <h1 class="text-3xl font-heading font-bold text-brand-text-primary">
        {getTimeOfDayGreeting()}
      </h1>
      <p class="text-sm text-brand-text-secondary mt-1">
        {collectionStore.stats.total_songs > 0
          ? i18n.t('home.libraryOverview', {
              songs: collectionStore.stats.total_songs,
              albums: collectionStore.stats.total_albums,
              artists: collectionStore.stats.total_artists
            })
          : i18n.t('home.exploreSub')}
      </p>
    </div>

    <div class="px-6 pt-4 space-y-12">
    {#if isLoading}
      <div class="flex items-center justify-center h-64">
        <div class="text-brand-text-secondary">{i18n.t('home.loading')}</div>
      </div>
    {:else}
      {#if topArtists.length > 0}
        <HorizontalScrollRow title={i18n.t('home.topArtists')} onHeaderClick={viewTopArtists}>
          {#each topArtists as artist (artist.name)}
            <div class="w-44 shrink-0 snap-start">
              <ArtistCard
                {artist}
                artistAlbums={getArtistAlbums(collectionStore.albums, artist.name)}
                artistSongs={getArtistSongs(collectionStore.songs, artist.name)}
                onclick={() => navigationStore.viewArtist(artist.name || "")}
              />
            </div>
          {/each}
        </HorizontalScrollRow>
      {/if}

      {#if frequentlyPlayed.length > 0 || featuredAlbums.length > 0 || recentlyAdded.length > 0}
        <div class="grid grid-cols-1 lg:grid-cols-2 gap-8">
          {#if frequentlyPlayed.length > 0}
            <HomeRowList
              title={i18n.t('home.mostPlayed')}
              items={frequentlyPlayed}
              variant="rank"
              onHeaderClick={() => navigationStore.viewAutoPlaylist({ kind: "most_played" })}
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

      {#if topArtists.length === 0 && frequentlyPlayed.length === 0 && recentlyAdded.length === 0}
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
