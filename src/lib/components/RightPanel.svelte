<script lang="ts">
  import { playerStore } from "../stores/player.svelte";
  import { themeStore } from "../stores/theme.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { Music, Clock } from "lucide-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { lyricsStatus } from "../utils/lyrics";
  import { invoke } from "@tauri-apps/api/core";
  import { prefs } from "../stores/prefs.svelte";
  import { deriveMoodmoji } from "../utils/moodmoji";

  interface Props {
    isOpen?: boolean;
    width?: number;
    onClose?: () => void;
  }

  let { isOpen = true, width = 288, onClose }: Props = $props();

  let currentSong = $derived(playerStore.currentSong);

  // Moodmoji: emoji hash derived from the current track's moodbar data
  let moodmoji = $state<string | null>(null);

  $effect(() => {
    const songId = currentSong?.id;
    moodmoji = null; // clear immediately when current track changes
    if (songId === undefined || !prefs.showMoodmoji) {
      return;
    }
    let cancelled = false;
    const timer = setTimeout(async () => {
      try {
        const data = await invoke<number[] | null>("get_moodbar_data", { songId });
        if (!cancelled) moodmoji = data ? deriveMoodmoji(data) : null;
      } catch (e) {
        if (!cancelled) moodmoji = null;
      }
    }, 300);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  });

  // Loudness normalization (#77) — expanded detail for the right panel
  // (the player bar only has room for a compact "R128"/"RG" badge).
  function loudnessSourceLabel(): string {
    switch (playerStore.loudnessSource) {
      case "analyzed": return i18n.t('playerBar.loudnessSourceAnalyzed', {}, 'R128 analysis');
      case "replay_gain": return i18n.t('playerBar.loudnessSourceReplayGain', {}, 'ReplayGain tag');
      case "fallback": return i18n.t('playerBar.loudnessSourceFallback', {}, 'Fallback gain');
      default: return "";
    }
  }

  let loudnessGainText = $derived.by(() => {
    const gain = playerStore.loudnessGainDb;
    if (gain === undefined) return "";
    return `${gain > 0 ? "+" : ""}${gain.toFixed(1)} dB`;
  });

  function lyricsStatusLabel(): string {
    if (!currentSong) return "";
    switch (lyricsStatus(currentSong)) {
      case "synced": return i18n.t('playerBar.lyricsSynced', {}, 'Synced (LRC)');
      case "plain": return i18n.t('playerBar.lyricsPlain', {}, 'Plain text');
      default: return i18n.t('playerBar.lyricsNone', {}, 'Not downloaded');
    }
  }
</script>

<aside
  style="width: {width}px;"
  class="relative bg-brand-sidebar flex flex-col h-full text-brand-text-secondary select-none flex-shrink-0 overflow-hidden"
  class:glass-surface={themeStore.isGlassTheme}
>
  <!-- Content -->
  <div class="flex-1 overflow-y-auto px-6 pt-6 pb-6 space-y-6">
    <!-- Current Song -->
    {#if currentSong}
      <div class="space-y-3">
        <div class="space-y-1">
          <p class="text-xs text-brand-text-secondary/60 uppercase tracking-wide">{i18n.t('playerBar.songLabel', {}, 'Song')}</p>
          <p class="text-sm font-semibold text-brand-text-primary truncate">{currentSong.title || i18n.t('collection.unknownSong')}</p>
        </div>

        <div class="space-y-1">
          <p class="text-xs text-brand-text-secondary/60 uppercase tracking-wide">{i18n.t('playerBar.artistLabel', {}, 'Artist')}</p>
          {#if currentSong.artist}
            <button
              onclick={() => collectionStore.viewArtist(currentSong.album_artist?.trim() || currentSong.artist || "")}
              class="text-sm text-brand-text-secondary hover:text-brand-accent-text hover:underline transition-all duration-150 truncate cursor-pointer text-left"
              title={i18n.t('collection.filterByArtist', { artist: currentSong.artist })}
            >
              {currentSong.artist}
            </button>
          {:else}
            <p class="text-sm text-brand-text-secondary truncate">{i18n.t('collection.unknownArtist')}</p>
          {/if}
        </div>

        <div class="space-y-1">
          <p class="text-xs text-brand-text-secondary/60 uppercase tracking-wide">{i18n.t('playerBar.albumLabel', {}, 'Album')}</p>
          <p class="text-sm text-brand-text-secondary truncate">{currentSong.album || i18n.t('collection.unknownAlbum')}</p>
        </div>
      </div>

      <!-- Moodmoji display -->
      {#if prefs.showMoodmoji && moodmoji}
        <div class="p-3.5 rounded-xl bg-brand-border/20 border border-brand-border/40 flex items-center justify-between shadow-xs">
          <div class="flex flex-col gap-0.5">
            <span class="text-xs font-semibold text-brand-text-primary uppercase tracking-wide">Moodmoji</span>
            <span class="text-[11px] text-brand-text-secondary/70">Audio mood signature</span>
          </div>
          <span
            class="text-3xl select-none leading-none tracking-widest"
            title={i18n.t('playerBar.moodmojiTooltip', {}, "Moodmoji — a mood hash derived from this track's dominant frequency bands and energy")}
          >
            {moodmoji}
          </span>
        </div>
      {/if}

      <!-- Playback Info -->
      <div class="space-y-2">
        {#if currentSong.filetype}
          <div class="flex items-center justify-between text-xs">
            <span class="text-brand-text-secondary/60">{i18n.t('playerBar.formatLabel', {}, 'Format')}</span>
            <span class="text-brand-text-primary uppercase">{currentSong.filetype}</span>
          </div>
        {/if}
        {#if currentSong.bitrate}
          <div class="flex items-center justify-between text-xs">
            <span class="text-brand-text-secondary/60">{i18n.t('playerBar.bitrateLabel', {}, 'Bitrate')}</span>
            <span class="text-brand-text-primary">{currentSong.bitrate} kbps</span>
          </div>
        {/if}
        {#if currentSong.samplerate}
          <div class="flex items-center justify-between text-xs">
            <span class="text-brand-text-secondary/60">{i18n.t('playerBar.sampleRateLabel', {}, 'Sample Rate')}</span>
            <span class="text-brand-text-primary">{(currentSong.samplerate / 1000).toFixed(1)} kHz</span>
          </div>
        {/if}
        {#if playerStore.loudnessSource !== "disabled"}
          <div class="flex items-center justify-between text-xs">
            <span class="text-brand-text-secondary/60">{i18n.t('playerBar.loudnessLabel', {}, 'Loudness')}</span>
            <span class="text-brand-text-primary">{loudnessSourceLabel()}{loudnessGainText ? ` · ${loudnessGainText}` : ""}</span>
          </div>
        {/if}
        <div class="flex items-center justify-between text-xs">
          <span class="text-brand-text-secondary/60">{i18n.t('playerBar.lyricsStatusLabel', {}, 'Lyrics')}</span>
          <span class="text-brand-text-primary">{lyricsStatusLabel()}</span>
        </div>
      </div>

      <!-- Additional Metadata -->
      <div class="space-y-2 text-xs">
        {#if currentSong.year}
          <div class="flex justify-between">
            <span class="text-brand-text-secondary/60">{i18n.t('playerBar.releasedLabel', {}, 'Released')}</span>
            <span class="text-brand-text-secondary">{currentSong.year}</span>
          </div>
        {/if}
        {#if currentSong.genre}
          <div class="flex justify-between">
            <span class="text-brand-text-secondary/60">{i18n.t('playerBar.genreLabel', {}, 'Genre')}</span>
            <span class="text-brand-text-secondary">{currentSong.genre}</span>
          </div>
        {/if}
        {#if currentSong.composer}
          <div class="flex justify-between">
            <span class="text-brand-text-secondary/60">{i18n.t('playerBar.composerLabel', {}, 'Composer')}</span>
            <span class="text-brand-text-secondary truncate">{currentSong.composer}</span>
          </div>
        {/if}
      </div>
    {:else}
      <div class="flex flex-col items-center justify-center h-full text-center">
        <Music class="w-12 h-12 text-brand-text-secondary/30 mb-3" />
        <p class="text-sm text-brand-text-secondary/60">{i18n.t('playerBar.notPlaying')}</p>
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
