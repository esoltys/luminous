<script lang="ts">
  import type { Song } from "../types";
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import CoverArt from "./CoverArt.svelte";
  import { Play, Plus } from "lucide-svelte";

  let { song }: { song: Song } = $props();

  let showAddToPlaylist = $state(false);

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

  async function handleAddToPlaylist(playlistId: number) {
    await playlistsStore.addToPlaylist(playlistId, song.id);
    showAddToPlaylist = false;
  }
</script>

<div class="flex-shrink-0 w-48 group">
  <!-- Card Container -->
  <div class="relative rounded-lg overflow-hidden bg-brand-sidebar border border-brand-border/50 transition-all duration-200 hover:border-brand-accent hover:shadow-lg hover:shadow-brand-accent/10">
    <!-- Cover Art -->
    <div class="relative aspect-square overflow-hidden bg-gray-900">
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

      <!-- Add to Playlist Button -->
      <div class="pt-2 flex gap-2">
        <div class="relative flex-1">
          <button
            onclick={() => (showAddToPlaylist = !showAddToPlaylist)}
            class="w-full flex items-center justify-center gap-1 bg-brand-accent/10 hover:bg-brand-accent/20 text-brand-accent rounded px-2 py-1.5 text-xs font-medium transition-colors"
            title="Add to playlist"
          >
            <Plus class="w-3.5 h-3.5" />
            <span>Add</span>
          </button>

          <!-- Playlist dropdown -->
          {#if showAddToPlaylist}
            <div class="absolute top-full left-0 right-0 mt-1 bg-brand-sidebar border border-brand-border rounded shadow-lg z-50 max-h-48 overflow-y-auto">
              {#if playlistsStore.playlists.length > 0}
                {#each playlistsStore.playlists as playlist}
                  <button
                    onclick={() => handleAddToPlaylist(playlist.id)}
                    class="w-full text-left px-3 py-2 text-xs text-brand-text-secondary hover:bg-brand-main hover:text-brand-text-primary transition-colors"
                  >
                    {playlist.name}
                  </button>
                {/each}
              {:else}
                <div class="px-3 py-2 text-xs text-brand-text-secondary/60">
                  No playlists yet
                </div>
              {/if}
            </div>
          {/if}
        </div>
      </div>
    </div>
  </div>
</div>

<style>
</style>
