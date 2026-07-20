<script lang="ts">
  import type { HomeItem } from "../types";
  import { playerStore } from "../stores/player.svelte";
  import CoverArt from "./CoverArt.svelte";
  import AlbumCard from "./AlbumCard.svelte";
  import { Play } from "lucide-svelte";
  import { i18n } from "../stores/i18n.svelte";

  let { item }: { item: HomeItem } = $props();

  function formatDuration(ns: number | undefined): string {
    if (!ns) return "0:00";
    const seconds = Math.floor(ns / 1_000_000_000);
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    return `${mins}:${secs.toString().padStart(2, "0")}`;
  }

  async function handlePlay(e: MouseEvent) {
    e.stopPropagation();
    if (item.type === "song") {
      await playerStore.playSong(item.song.id);
    }
  }
</script>

{#if item.type === "album"}
  <AlbumCard album={item.album} widthClass="w-48 shrink-0" />
{:else}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="flex-shrink-0 w-48 group relative">
    <!-- Card Container -->
    <div class="relative rounded-lg overflow-hidden bg-brand-sidebar border border-brand-border/50 transition-all duration-200 hover:border-brand-accent hover:shadow-lg hover:shadow-brand-accent/10">
      <!-- Cover Art -->
      <div class="relative aspect-square overflow-hidden bg-brand-sidebar">
        <CoverArt
          songId={item.song.id}
          artEmbedded={item.song.art_embedded}
          artAutomatic={item.song.art_automatic}
          artManual={item.song.art_manual}
          sizeClass="w-full h-full"
        />

        <!-- Play Button Overlay -->
        <div class="absolute inset-0 bg-black/0 group-hover:bg-black/40 transition-colors duration-200 flex items-center justify-center">
          <button
            onclick={handlePlay}
            class="opacity-0 group-hover:opacity-100 transition-opacity duration-200 bg-brand-accent hover:bg-brand-accent-hover rounded-full p-3 text-brand-accent-contrast shadow-lg cursor-pointer"
            title={i18n.t('playerBar.play')}
          >
            <Play class="w-6 h-6 fill-current" />
          </button>
        </div>
      </div>

      <!-- Metadata -->
      <div class="p-3 space-y-2">
        <!-- Song Title -->
        <h3 class="text-sm font-semibold text-brand-text-primary truncate" title={item.song.title}>
          {item.song.title || i18n.t('collection.unknownSong')}
        </h3>

        <!-- Song Artist -->
        <p class="text-xs text-brand-text-secondary truncate" title={item.song.artist}>
          {item.song.artist || i18n.t('collection.unknownArtist')}
        </p>

        <!-- Song Duration -->
        <p class="text-xs text-brand-text-secondary/60">
          {formatDuration(item.song.length_nanosec)}
        </p>
      </div>
    </div>
  </div>
{/if}

<style>
</style>
