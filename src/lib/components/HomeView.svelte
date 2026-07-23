<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { playerStore } from "../stores/player.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import type { HomeItem, ArtistItem } from "../types";
  import { getArtistAlbums, getArtistSongs } from "../utils/artist";
  import HorizontalScrollRow from "./HorizontalScrollRow.svelte";
  import ArtistCard from "./ArtistCard.svelte";
  import HomeRowList from "./HomeRowList.svelte";
  import { Disc3 } from "lucide-svelte";
  import { i18n } from "../stores/i18n.svelte";

  let topArtists = $state<ArtistItem[]>([]);
  let frequentlyPlayed = $state<HomeItem[]>([]);
  let recentlyAdded = $state<HomeItem[]>([]);
  let isLoading = $state(true);

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
      const [artists, frequent, added] = await Promise.all([
        invoke<ArtistItem[]>("get_top_artists", { limit: 15 }),
        invoke<HomeItem[]>("get_most_frequently_played", { limit: 5 }),
        invoke<HomeItem[]>("get_recently_added", { limit: 5 }),
      ]);
      topArtists = artists;
      frequentlyPlayed = frequent;
      recentlyAdded = added;
    } catch (err) {
      console.error("Failed to load curated data:", err);
    } finally {
      isLoading = false;
    }
  }

  onMount(() => {
    loadCuratedData();
  });
</script>

<div class="flex flex-col h-full w-full bg-brand-main overflow-hidden">
  <!-- Content Area -->
  <div class="flex-1 overflow-y-auto" class:pb-28={!!playerStore.currentSong}>
    <!-- Header -->
    <div class="px-6 pt-8">
      <h1 class="text-3xl font-bold text-brand-text-primary">
        {getTimeOfDayGreeting()}
      </h1>
      <p class="text-sm text-brand-text-secondary mt-1">
        {i18n.t('home.exploreSub')}
      </p>
    </div>

    <div class="px-6 pt-4 space-y-12">
    {#if isLoading}
      <div class="flex items-center justify-center h-64">
        <div class="text-brand-text-secondary">{i18n.t('home.loading')}</div>
      </div>
    {:else}
      <!-- Top Artists Section -->
      {#if topArtists.length > 0}
        <HorizontalScrollRow title={i18n.t('home.topArtists')}>
          {#each topArtists as artist (artist.name)}
            <div class="w-44 shrink-0 snap-start">
              <ArtistCard
                {artist}
                artistAlbums={getArtistAlbums(collectionStore.albums, artist.name)}
                artistSongs={getArtistSongs(collectionStore.songs, artist.name)}
                onclick={() => collectionStore.viewArtist(artist.name || "")}
              />
            </div>
          {/each}
        </HorizontalScrollRow>
      {/if}

      <!-- Most Played / Recently Added Row Columns -->
      {#if frequentlyPlayed.length > 0 || recentlyAdded.length > 0}
        <div class="grid grid-cols-1 lg:grid-cols-2 gap-8">
          {#if frequentlyPlayed.length > 0}
            <HomeRowList title={i18n.t('home.mostPlayed')} items={frequentlyPlayed} variant="rank" />
          {/if}
          {#if recentlyAdded.length > 0}
            <HomeRowList title={i18n.t('home.recentlyAdded')} items={recentlyAdded} variant="added" />
          {/if}
        </div>
      {/if}

      <!-- Empty State -->
      {#if topArtists.length === 0 && frequentlyPlayed.length === 0 && recentlyAdded.length === 0}
        <div class="flex flex-col items-center justify-center h-64 text-center">
          <Disc3 class="w-16 h-16 text-brand-text-secondary/30 mb-4" />
          <p class="text-brand-text-secondary">
            {i18n.t('home.emptyState')}
          </p>
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
