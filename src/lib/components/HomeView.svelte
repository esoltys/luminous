<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { playerStore } from "../stores/player.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import type { Song } from "../types";
  import CurationCarousel from "./CurationCarousel.svelte";
  import { Disc3 } from "lucide-svelte";

  let recentlyPlayed = $state<Song[]>([]);
  let frequentlyPlayed = $state<Song[]>([]);
  let recentlyAdded = $state<Song[]>([]);
  let isLoading = $state(true);

  function getTimeOfDayGreeting(): string {
    const hour = new Date().getHours();
    if (hour >= 5 && hour < 12) return "Good Morning";
    if (hour >= 12 && hour < 17) return "Good Afternoon";
    if (hour >= 17 && hour < 21) return "Good Evening";
    return "Good Night";
  }

  async function loadCuratedData() {
    isLoading = true;
    try {
      const [recent, frequent, added] = await Promise.all([
        invoke<Song[]>("get_recently_played", { limit: 10 }),
        invoke<Song[]>("get_most_frequently_played", { limit: 10 }),
        invoke<Song[]>("get_recently_added", { limit: 10 }),
      ]);
      recentlyPlayed = recent;
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
  <!-- Header -->
  <div class="flex-shrink-0 px-6 py-8 border-b border-brand-border">
    <h1 class="text-3xl font-bold text-brand-text-primary">
      {getTimeOfDayGreeting()}
    </h1>
    <p class="text-sm text-brand-text-secondary mt-1">
      Explore your music collection
    </p>
  </div>

  <!-- Content Area -->
  <div class="flex-1 overflow-y-auto px-6 py-8 pb-24 space-y-12">
    {#if isLoading}
      <div class="flex items-center justify-center h-64">
        <div class="text-brand-text-secondary">Loading your collection...</div>
      </div>
    {:else}
      <!-- Recently Played Section -->
      {#if recentlyPlayed.length > 0}
        <div>
          <h2 class="text-xl font-semibold text-brand-text-primary mb-4">
            Recently Played
          </h2>
          <CurationCarousel songs={recentlyPlayed} />
        </div>
      {/if}

      <!-- Most Frequently Played Section -->
      {#if frequentlyPlayed.length > 0}
        <div>
          <h2 class="text-xl font-semibold text-brand-text-primary mb-4">
            Most Played
          </h2>
          <CurationCarousel songs={frequentlyPlayed} />
        </div>
      {/if}

      <!-- Recently Added Section -->
      {#if recentlyAdded.length > 0}
        <div>
          <h2 class="text-xl font-semibold text-brand-text-primary mb-4">
            Recently Added
          </h2>
          <CurationCarousel songs={recentlyAdded} />
        </div>
      {/if}

      <!-- Empty State -->
      {#if recentlyPlayed.length === 0 && frequentlyPlayed.length === 0 && recentlyAdded.length === 0}
        <div class="flex flex-col items-center justify-center h-64 text-center">
          <Disc3 class="w-16 h-16 text-brand-text-secondary/30 mb-4" />
          <p class="text-brand-text-secondary">
            Start adding music to see your personalized collections
          </p>
        </div>
      {/if}
    {/if}
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
