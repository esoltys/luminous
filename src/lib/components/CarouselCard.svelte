<script lang="ts">
  import type { Song } from "../types";
  import { playerStore } from "../stores/player.svelte";
  import CoverArt from "./CoverArt.svelte";
  import { Play } from "lucide-svelte";

  let { song }: { song: Song } = $props();

  function formatDuration(ns: number | undefined): string {
    if (!ns) return "0:00";
    const seconds = Math.floor(ns / 1_000_000_000);
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    return `${mins}:${secs.toString().padStart(2, "0")}`;
  }

  async function handlePlay() {
    await playerStore.playSong(song.id);
  }
</script>

<div class="flex-shrink-0 w-48 group relative">
  <!-- Card Container -->
  <div class="relative rounded-lg overflow-hidden bg-brand-sidebar border border-brand-border/50 transition-all duration-200 hover:border-brand-accent hover:shadow-lg hover:shadow-brand-accent/10">
    <!-- Cover Art -->
    <div class="relative aspect-square overflow-hidden bg-brand-sidebar">
      <CoverArt
        songId={song.id}
        artEmbedded={song.art_embedded}
        artAutomatic={song.art_automatic}
        artManual={song.art_manual}
        sizeClass="w-full h-full"
      />

      <!-- Play Button Overlay -->
      <div class="absolute inset-0 bg-black/0 group-hover:bg-black/40 transition-colors duration-200 flex items-center justify-center">
        <button
          onclick={handlePlay}
          class="opacity-0 group-hover:opacity-100 transition-opacity duration-200 bg-brand-accent hover:bg-brand-accent-hover rounded-full p-3 text-white shadow-lg"
          title="Play"
        >
          <Play class="w-6 h-6 fill-current" />
        </button>
      </div>
    </div>

    <!-- Metadata -->
    <div class="p-3 space-y-2">
      <!-- Title -->
      <h3 class="text-sm font-semibold text-brand-text-primary truncate">
        {song.title || "Unknown Track"}
      </h3>

      <!-- Artist -->
      <p class="text-xs text-brand-text-secondary truncate">
        {song.artist || "Unknown Artist"}
      </p>

      <!-- Duration -->
      <p class="text-xs text-brand-text-secondary/60">
        {formatDuration(song.length_nanosec)}
      </p>
    </div>
  </div>
</div>

<style>
</style>
