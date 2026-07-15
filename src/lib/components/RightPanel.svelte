<script lang="ts">
  import { playerStore } from "../stores/player.svelte";
  import { themeStore } from "../stores/theme.svelte";
  import { Music, Clock } from "lucide-svelte";

  interface Props {
    isOpen?: boolean;
    width?: number;
    onClose?: () => void;
  }

  let { isOpen = true, width = 288, onClose }: Props = $props();

  let currentSong = $derived(playerStore.currentSong);
</script>

<aside
  style="width: {width}px;"
  class="relative bg-brand-sidebar border-l border-brand-border flex flex-col h-full text-brand-text-secondary select-none flex-shrink-0 overflow-hidden rounded-tl-2xl rounded-bl-2xl"
  class:glass-surface={themeStore.isGlassTheme}
>
  <!-- Header -->
  <div class="h-20 flex items-center px-6 border-b border-brand-border">
    <h2 class="text-lg font-bold text-brand-text-primary flex items-center gap-2">
      <Music class="w-5 h-5" />
      Now Playing
    </h2>
  </div>

  <!-- Content -->
  <div class="flex-1 overflow-y-auto p-6 space-y-6">
    <!-- Current Song -->
    {#if currentSong}
      <div class="space-y-3">
        <div class="space-y-1">
          <p class="text-xs text-brand-text-secondary/60 uppercase tracking-wide">Track</p>
          <p class="text-sm font-semibold text-brand-text-primary truncate">{currentSong.title || "Unknown"}</p>
        </div>

        <div class="space-y-1">
          <p class="text-xs text-brand-text-secondary/60 uppercase tracking-wide">Artist</p>
          <p class="text-sm text-brand-text-secondary truncate">{currentSong.artist || "Unknown Artist"}</p>
        </div>

        <div class="space-y-1">
          <p class="text-xs text-brand-text-secondary/60 uppercase tracking-wide">Album</p>
          <p class="text-sm text-brand-text-secondary truncate">{currentSong.album || "Unknown Album"}</p>
        </div>
      </div>

      <!-- Playback Info -->
      <div class="space-y-2 bg-brand-main/40 rounded-lg p-3">
        {#if currentSong.bitrate}
          <div class="flex items-center justify-between text-xs">
            <span class="text-brand-text-secondary/60">Bitrate</span>
            <span class="text-brand-text-primary">{currentSong.bitrate} kbps</span>
          </div>
        {/if}
        {#if currentSong.samplerate}
          <div class="flex items-center justify-between text-xs">
            <span class="text-brand-text-secondary/60">Sample Rate</span>
            <span class="text-brand-text-primary">{(currentSong.samplerate / 1000).toFixed(1)} kHz</span>
          </div>
        {/if}
      </div>

      <!-- Additional Metadata -->
      <div class="space-y-2 text-xs">
        {#if currentSong.year}
          <div class="flex justify-between">
            <span class="text-brand-text-secondary/60">Released</span>
            <span class="text-brand-text-secondary">{currentSong.year}</span>
          </div>
        {/if}
        {#if currentSong.genre}
          <div class="flex justify-between">
            <span class="text-brand-text-secondary/60">Genre</span>
            <span class="text-brand-text-secondary">{currentSong.genre}</span>
          </div>
        {/if}
        {#if currentSong.composer}
          <div class="flex justify-between">
            <span class="text-brand-text-secondary/60">Composer</span>
            <span class="text-brand-text-secondary truncate">{currentSong.composer}</span>
          </div>
        {/if}
      </div>
    {:else}
      <div class="flex flex-col items-center justify-center h-full text-center">
        <Music class="w-12 h-12 text-brand-text-secondary/30 mb-3" />
        <p class="text-sm text-brand-text-secondary/60">No track playing</p>
      </div>
    {/if}
  </div>


</aside>

<style>
  aside {
    scrollbar-width: thin;
    scrollbar-color: var(--color-border) transparent;
  }

  aside ::-webkit-scrollbar {
    width: 6px;
  }

  aside ::-webkit-scrollbar-track {
    background: transparent;
  }

  aside ::-webkit-scrollbar-thumb {
    background: var(--color-border);
    border-radius: 3px;
  }
</style>
